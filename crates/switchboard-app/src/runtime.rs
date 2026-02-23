use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{Display, Formatter, Result as FmtResult};

#[cfg(test)]
use std::convert::Infallible;
#[cfg(test)]
use switchboard_core::NoopPersistence;
use switchboard_core::{
    BrowserState, Engine, EngineError, Intent, Patch, ProfileId, SettingValue, TabId,
    TabRuntimeState, WorkspaceId,
};

use crate::bridge::UiCommand;
use crate::host::{
    install_content_event_handler, install_ui_command_handler, install_ui_state_provider,
    install_window_event_handler, CefHost, ContentEvent, ContentViewId, UiViewId, WindowEvent,
    WindowId, WindowSize,
};
#[cfg(not(test))]
use crate::persistence::{AppPersistence, AppPersistenceError};

const UI_SHELL_URL_BASE: &str = "app://ui";
const THUMBNAIL_MAX_ENTRIES: usize = 120;
const WINDOW_WIDTH_SETTING_KEY: &str = "window.width";
const WINDOW_HEIGHT_SETTING_KEY: &str = "window.height";
const SEARCH_ENGINE_SETTING_KEY: &str = "search_engine";
const HOMEPAGE_SETTING_KEY: &str = "homepage";
const NEW_TAB_BEHAVIOR_SETTING_KEY: &str = "new_tab_behavior";
const NEW_TAB_CUSTOM_URL_SETTING_KEY: &str = "new_tab_custom_url";
const KEYBINDING_CLOSE_TAB_SETTING_KEY: &str = "keybinding_close_tab";
const KEYBINDING_COMMAND_PALETTE_SETTING_KEY: &str = "keybinding_command_palette";
const KEYBINDING_FOCUS_NAVIGATION_SETTING_KEY: &str = "keybinding_focus_navigation";
const KEYBINDING_TOGGLE_DEVTOOLS_SETTING_KEY: &str = "keybinding_toggle_devtools";
const PASSWORD_MANAGER_DEFAULT_PROVIDER_SETTING_KEY: &str = "password_manager.default_provider";
const PASSWORD_MANAGER_DEFAULT_AUTOFILL_SETTING_KEY: &str = "password_manager.default_autofill";
const PASSWORD_MANAGER_DEFAULT_SAVE_PROMPT_SETTING_KEY: &str =
    "password_manager.default_save_prompt";
const PASSWORD_MANAGER_DEFAULT_FALLBACK_SETTING_KEY: &str = "password_manager.default_fallback";
const WINDOW_MIN_WIDTH: u32 = 640;
const WINDOW_MIN_HEIGHT: u32 = 480;

#[cfg(test)]
type RuntimePersistence = NoopPersistence;
#[cfg(not(test))]
type RuntimePersistence = AppPersistence;

#[cfg(test)]
type RuntimePersistenceError = Infallible;
#[cfg(not(test))]
type RuntimePersistenceError = AppPersistenceError;

#[derive(Debug)]
#[cfg_attr(test, allow(dead_code))]
pub enum RuntimeError<HError> {
    PersistenceInit(String),
    Host(HError),
    Engine(EngineError<RuntimePersistenceError>),
    NoActiveWorkspace,
    NoActiveProfile,
    BlockedContentNavigation(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ContentBinding {
    view_id: ContentViewId,
    profile_id: ProfileId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LiveTabBinding {
    content: ContentBinding,
    last_url: String,
}

pub struct AppRuntime<H: CefHost> {
    engine: Engine<RuntimePersistence>,
    host: H,
    window_id: WindowId,
    ui_view_id: UiViewId,
    default_workspace_id: WorkspaceId,
    tab_bindings: BTreeMap<TabId, LiveTabBinding>,
    thumbnail_lru: Vec<TabId>,
}

impl<H: CefHost + 'static> AppRuntime<H> {
    pub fn bootstrap(mut host: H, ui_version: &str) -> Result<Self, RuntimeError<H::Error>> {
        #[cfg(test)]
        let (persistence, mut state) = (NoopPersistence, BrowserState::default());

        #[cfg(not(test))]
        let (persistence, mut state) = {
            let mut persistence = AppPersistence::open_default()
                .map_err(|error| RuntimeError::PersistenceInit(error.to_string()))?;
            let state = persistence
                .load_state()
                .map_err(|error| RuntimeError::PersistenceInit(error.to_string()))?
                .unwrap_or_default();
            (persistence, state)
        };

        let workspace_id = ensure_bootstrap_state(&mut state);
        let initial_window_size = restored_window_size(&state);
        let mut engine = Engine::with_state(persistence, state, 0);

        let window_id = host
            .create_window("Switchboard", initial_window_size)
            .map_err(RuntimeError::Host)?;
        let ui_shell_url = format!("{UI_SHELL_URL_BASE}?v={}", std::process::id());
        let ui_view_id = host
            .create_ui_view(window_id, &ui_shell_url)
            .map_err(RuntimeError::Host)?;

        let ui_ready = UiCommand::UiReady {
            ui_version: ui_version.to_owned(),
        };
        engine
            .dispatch(ui_ready.into_intent())
            .map_err(RuntimeError::Engine)?;

        Ok(Self {
            engine,
            host,
            window_id,
            ui_view_id,
            default_workspace_id: workspace_id,
            tab_bindings: BTreeMap::new(),
            thumbnail_lru: Vec::new(),
        })
    }

    pub fn default_workspace_id(&self) -> WorkspaceId {
        self.default_workspace_id
    }

    pub fn ui_view_id(&self) -> UiViewId {
        self.ui_view_id
    }

    pub fn revision(&self) -> u64 {
        self.engine.revision()
    }

    pub fn engine(&self) -> &Engine<RuntimePersistence> {
        &self.engine
    }

    pub fn has_tabs(&self) -> bool {
        !self.engine.state().tabs.is_empty()
    }

    #[cfg(test)]
    pub fn host(&self) -> &H {
        &self.host
    }

    pub fn run(mut self) -> Result<(), RuntimeError<H::Error>>
    where
        H::Error: Display,
    {
        self.sync_runtime_views()?;

        let runtime_ptr: *mut Self = &mut self;
        install_ui_command_handler(Some(Box::new(move |command| unsafe {
            if let Err(error) = (*runtime_ptr).handle_ui_command(command) {
                eprintln!("switchboard-app: UI command failed: {error}");
            }
        })));
        install_ui_state_provider(Some(Box::new(move || unsafe {
            (*runtime_ptr).ui_shell_state_json()
        })));
        install_content_event_handler(Some(Box::new(move |event| unsafe {
            if let Err(error) = (*runtime_ptr).handle_content_event(event) {
                eprintln!("switchboard-app: content event failed: {error}");
            }
        })));
        install_window_event_handler(Some(Box::new(move |event| unsafe {
            if let Err(error) = (*runtime_ptr).handle_window_event(event) {
                eprintln!("switchboard-app: window event failed: {error}");
            }
        })));

        let result = self.host.run_event_loop().map_err(RuntimeError::Host);
        install_ui_command_handler(None);
        install_ui_state_provider(None);
        install_content_event_handler(None);
        install_window_event_handler(None);
        result
    }

    pub fn handle_ui_command(
        &mut self,
        command: UiCommand,
    ) -> Result<Patch, RuntimeError<H::Error>> {
        match command {
            UiCommand::NavigateActive { url } => {
                if let Some(tab_id) = self.resolve_active_tab_id() {
                    return self.handle_intent(Intent::Navigate { tab_id, url });
                }
                let workspace_id = self
                    .resolve_active_workspace_id()
                    .ok_or(RuntimeError::NoActiveWorkspace)?;
                self.handle_intent(Intent::NewTab {
                    workspace_id,
                    url: Some(url),
                    make_active: true,
                })
            }
            UiCommand::NewWorkspace { name } => {
                let profile_id = self
                    .resolve_active_profile_id()
                    .ok_or(RuntimeError::NoActiveProfile)?;
                self.handle_intent(Intent::NewWorkspace { profile_id, name })
            }
            UiCommand::NewProfile { name } => self.handle_intent(Intent::NewProfile { name }),
            UiCommand::ToggleDevTools => {
                self.host
                    .toggle_dev_tools_for_active_content()
                    .map_err(RuntimeError::Host)?;
                let revision = self.revision();
                Ok(Patch {
                    ops: Vec::new(),
                    from_revision: revision,
                    to_revision: revision,
                })
            }
            other => self.handle_intent(other.into_intent()),
        }
    }

    pub fn handle_intent(&mut self, intent: Intent) -> Result<Patch, RuntimeError<H::Error>> {
        if let Intent::Navigate { url, .. } = &intent {
            if url.starts_with("app://") {
                return Err(RuntimeError::BlockedContentNavigation(url.clone()));
            }
        }

        let patch = self.engine.dispatch(intent).map_err(RuntimeError::Engine)?;
        self.sync_runtime_views()?;
        Ok(patch)
    }

    pub fn handle_content_event(
        &mut self,
        event: ContentEvent,
    ) -> Result<Patch, RuntimeError<H::Error>> {
        let (intent, tab_id, should_capture_thumbnail) = match event {
            ContentEvent::UrlChanged { tab_id, url } => {
                (Intent::ObserveTabUrl { tab_id, url }, tab_id, false)
            }
            ContentEvent::TitleChanged { tab_id, title } => {
                (Intent::ObserveTabTitle { tab_id, title }, tab_id, false)
            }
            ContentEvent::LoadingChanged { tab_id, is_loading } => (
                Intent::ObserveTabLoading { tab_id, is_loading },
                tab_id,
                !is_loading,
            ),
        };

        if !self.engine.state().tabs.contains_key(&tab_id) {
            let revision = self.revision();
            return Ok(Patch {
                ops: Vec::new(),
                from_revision: revision,
                to_revision: revision,
            });
        }

        let patch = self.handle_intent(intent)?;
        if should_capture_thumbnail {
            self.capture_thumbnail_for_tab(tab_id)?;
            self.cleanup_thumbnail_storage()?;
        }
        Ok(patch)
    }

    pub fn handle_window_event(
        &mut self,
        event: WindowEvent,
    ) -> Result<Patch, RuntimeError<H::Error>> {
        match event {
            WindowEvent::Resized { width, height } => self.persist_window_size(width, height),
        }
    }

    pub fn active_tab_id(&self, workspace_id: WorkspaceId) -> Option<TabId> {
        self.engine
            .state()
            .workspaces
            .get(&workspace_id)
            .and_then(|workspace| workspace.active_tab_id)
    }

    fn resolve_active_tab_id(&self) -> Option<TabId> {
        let state = self.engine.state();
        let profile_id = state.active_profile_id?;
        let workspace_id = state.profiles.get(&profile_id)?.active_workspace_id?;
        state.workspaces.get(&workspace_id)?.active_tab_id
    }

    fn resolve_active_workspace_id(&self) -> Option<WorkspaceId> {
        let state = self.engine.state();
        let profile_id = state.active_profile_id?;
        state.profiles.get(&profile_id)?.active_workspace_id
    }

    fn resolve_active_profile_id(&self) -> Option<ProfileId> {
        self.engine.state().active_profile_id
    }

    fn persist_window_size(
        &mut self,
        width: u32,
        height: u32,
    ) -> Result<Patch, RuntimeError<H::Error>> {
        let width = width.max(WINDOW_MIN_WIDTH);
        let height = height.max(WINDOW_MIN_HEIGHT);
        let width_value = i64::from(width);
        let height_value = i64::from(height);

        let state = self.engine.state();
        let width_changed = setting_int(state, WINDOW_WIDTH_SETTING_KEY) != Some(width_value);
        let height_changed = setting_int(state, WINDOW_HEIGHT_SETTING_KEY) != Some(height_value);
        if !width_changed && !height_changed {
            let revision = self.revision();
            return Ok(Patch {
                ops: Vec::new(),
                from_revision: revision,
                to_revision: revision,
            });
        }

        let mut patch = Patch {
            ops: Vec::new(),
            from_revision: self.revision(),
            to_revision: self.revision(),
        };
        if width_changed {
            patch = self.handle_intent(Intent::SettingSet {
                key: WINDOW_WIDTH_SETTING_KEY.to_owned(),
                value: SettingValue::Int(width_value),
            })?;
        }
        if height_changed {
            patch = self.handle_intent(Intent::SettingSet {
                key: WINDOW_HEIGHT_SETTING_KEY.to_owned(),
                value: SettingValue::Int(height_value),
            })?;
        }
        Ok(patch)
    }

    fn sync_runtime_views(&mut self) -> Result<(), RuntimeError<H::Error>> {
        let active_profile_id = self.resolve_active_profile_id();
        let active_tab_id = self.resolve_active_tab_id();

        let desired_live_tabs: Vec<(TabId, ProfileId, String)> = self
            .engine
            .state()
            .tabs
            .values()
            .filter(|tab| {
                matches!(
                    tab.runtime_state,
                    TabRuntimeState::Active | TabRuntimeState::Warm
                )
            })
            .map(|tab| (tab.id, tab.profile_id, tab.url.clone()))
            .collect();
        let desired_live_ids: BTreeSet<TabId> = desired_live_tabs
            .iter()
            .map(|(tab_id, _, _)| *tab_id)
            .collect();

        let stale_tabs: Vec<TabId> = self
            .tab_bindings
            .keys()
            .copied()
            .filter(|tab_id| !desired_live_ids.contains(tab_id))
            .collect();
        for tab_id in stale_tabs {
            if let Some(binding) = self.tab_bindings.remove(&tab_id) {
                self.host
                    .destroy_content_view(binding.content.view_id)
                    .map_err(RuntimeError::Host)?;
            }
        }

        for (tab_id, profile_id, url) in desired_live_tabs {
            match self.tab_bindings.get(&tab_id).cloned() {
                Some(existing) => {
                    if existing.last_url != url {
                        self.host
                            .navigate_content_view(existing.content.view_id, tab_id, &url)
                            .map_err(RuntimeError::Host)?;
                        if let Some(binding) = self.tab_bindings.get_mut(&tab_id) {
                            binding.last_url = url.clone();
                            binding.content.profile_id = profile_id;
                        }
                    }
                }
                None => {
                    let view_id = self
                        .host
                        .create_content_view(self.window_id, tab_id, &url)
                        .map_err(RuntimeError::Host)?;
                    self.tab_bindings.insert(
                        tab_id,
                        LiveTabBinding {
                            content: ContentBinding {
                                view_id,
                                profile_id,
                            },
                            last_url: url,
                        },
                    );
                }
            }
        }

        let visibility: Vec<(TabId, ContentBinding)> = self
            .tab_bindings
            .iter()
            .map(|(tab_id, binding)| (*tab_id, binding.content))
            .collect();
        for (tab_id, binding) in visibility {
            let visible =
                Some(tab_id) == active_tab_id && Some(binding.profile_id) == active_profile_id;
            self.host
                .set_content_view_visible(binding.view_id, visible)
                .map_err(RuntimeError::Host)?;
        }

        Ok(())
    }

    fn capture_thumbnail_for_tab(&mut self, tab_id: TabId) -> Result<(), RuntimeError<H::Error>> {
        let Some(tab) = self.engine.state().tabs.get(&tab_id).cloned() else {
            self.thumbnail_lru.retain(|candidate| *candidate != tab_id);
            return Ok(());
        };
        let data_url = build_thumbnail_data_url(&tab.title, &tab.url);
        self.engine
            .dispatch(Intent::ObserveTabThumbnail {
                tab_id,
                data_url: Some(data_url),
            })
            .map_err(RuntimeError::Engine)?;
        self.touch_thumbnail_lru(tab_id);
        Ok(())
    }

    fn touch_thumbnail_lru(&mut self, tab_id: TabId) {
        if let Some(index) = self
            .thumbnail_lru
            .iter()
            .position(|candidate| *candidate == tab_id)
        {
            self.thumbnail_lru.remove(index);
        }
        self.thumbnail_lru.push(tab_id);
    }

    fn cleanup_thumbnail_storage(&mut self) -> Result<(), RuntimeError<H::Error>> {
        self.thumbnail_lru.retain(|tab_id| {
            self.engine
                .state()
                .tabs
                .get(tab_id)
                .and_then(|tab| tab.thumbnail_data_url.as_ref())
                .is_some()
        });

        while self.thumbnail_lru.len() > THUMBNAIL_MAX_ENTRIES {
            let tab_id = self.thumbnail_lru.remove(0);
            let has_thumbnail = self
                .engine
                .state()
                .tabs
                .get(&tab_id)
                .and_then(|tab| tab.thumbnail_data_url.as_ref())
                .is_some();
            if !has_thumbnail {
                continue;
            }
            self.engine
                .dispatch(Intent::ObserveTabThumbnail {
                    tab_id,
                    data_url: None,
                })
                .map_err(RuntimeError::Engine)?;
        }
        Ok(())
    }

    pub fn ui_shell_state_json(&self) -> String {
        let state = self.engine.state();
        let mut json = String::new();
        json.push('{');
        json.push_str("\"revision\":");
        json.push_str(&self.revision().to_string());
        json.push(',');
        json.push_str("\"active_profile_id\":");
        match state.active_profile_id {
            Some(profile_id) => json.push_str(&profile_id.0.to_string()),
            None => json.push_str("null"),
        }
        json.push(',');
        json.push_str("\"profiles\":[");
        let mut first = true;
        for profile in state.profiles.values() {
            if !first {
                json.push(',');
            }
            first = false;
            json.push('{');
            json.push_str("\"id\":");
            json.push_str(&profile.id.0.to_string());
            json.push(',');
            json.push_str("\"name\":");
            push_json_string(&mut json, &profile.name);
            json.push(',');
            json.push_str("\"active_workspace_id\":");
            match profile.active_workspace_id {
                Some(workspace_id) => json.push_str(&workspace_id.0.to_string()),
                None => json.push_str("null"),
            }
            json.push(',');
            json.push_str("\"workspace_order\":[");
            for (index, workspace_id) in profile.workspace_order.iter().enumerate() {
                if index > 0 {
                    json.push(',');
                }
                json.push_str(&workspace_id.0.to_string());
            }
            json.push_str("]}");
        }
        json.push_str("],");
        json.push_str("\"workspaces\":[");
        let mut first = true;
        for workspace in state.workspaces.values() {
            if !first {
                json.push(',');
            }
            first = false;
            json.push('{');
            json.push_str("\"id\":");
            json.push_str(&workspace.id.0.to_string());
            json.push(',');
            json.push_str("\"profile_id\":");
            json.push_str(&workspace.profile_id.0.to_string());
            json.push(',');
            json.push_str("\"name\":");
            push_json_string(&mut json, &workspace.name);
            json.push(',');
            json.push_str("\"active_tab_id\":");
            match workspace.active_tab_id {
                Some(tab_id) => json.push_str(&tab_id.0.to_string()),
                None => json.push_str("null"),
            }
            json.push(',');
            json.push_str("\"tab_order\":[");
            for (index, tab_id) in workspace.tab_order.iter().enumerate() {
                if index > 0 {
                    json.push(',');
                }
                json.push_str(&tab_id.0.to_string());
            }
            json.push_str("]}");
        }
        json.push_str("],");
        json.push_str("\"tabs\":[");
        let mut first = true;
        for tab in state.tabs.values() {
            if !first {
                json.push(',');
            }
            first = false;
            json.push('{');
            json.push_str("\"id\":");
            json.push_str(&tab.id.0.to_string());
            json.push(',');
            json.push_str("\"profile_id\":");
            json.push_str(&tab.profile_id.0.to_string());
            json.push(',');
            json.push_str("\"workspace_id\":");
            json.push_str(&tab.workspace_id.0.to_string());
            json.push(',');
            json.push_str("\"url\":");
            push_json_string(&mut json, &tab.url);
            json.push(',');
            json.push_str("\"title\":");
            push_json_string(&mut json, &tab.title);
            json.push(',');
            json.push_str("\"loading\":");
            json.push_str(if tab.loading { "true" } else { "false" });
            json.push(',');
            json.push_str("\"thumbnail_data_url\":");
            match &tab.thumbnail_data_url {
                Some(value) => push_json_string(&mut json, value),
                None => json.push_str("null"),
            }
            json.push_str("}");
        }
        json.push_str("],");
        json.push_str("\"settings\":{");
        let mut first = true;
        for (key, value) in &state.settings {
            if !first {
                json.push(',');
            }
            first = false;
            push_json_string(&mut json, key);
            json.push(':');
            push_json_setting_value(&mut json, value);
        }
        json.push_str("}}");
        json
    }
}

fn restored_window_size(state: &BrowserState) -> WindowSize {
    let defaults = WindowSize::default();
    let width = setting_int(state, WINDOW_WIDTH_SETTING_KEY)
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(defaults.width)
        .max(WINDOW_MIN_WIDTH);
    let height = setting_int(state, WINDOW_HEIGHT_SETTING_KEY)
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(defaults.height)
        .max(WINDOW_MIN_HEIGHT);
    WindowSize { width, height }
}

fn setting_int(state: &BrowserState, key: &str) -> Option<i64> {
    match state.settings.get(key) {
        Some(SettingValue::Int(value)) => Some(*value),
        _ => None,
    }
}

fn ensure_default_settings(state: &mut BrowserState) {
    state
        .settings
        .entry(SEARCH_ENGINE_SETTING_KEY.to_owned())
        .or_insert_with(|| SettingValue::Text("google".to_owned()));
    state
        .settings
        .entry(HOMEPAGE_SETTING_KEY.to_owned())
        .or_insert_with(|| SettingValue::Text("https://youtube.com".to_owned()));
    state
        .settings
        .entry(NEW_TAB_BEHAVIOR_SETTING_KEY.to_owned())
        .or_insert_with(|| SettingValue::Text("homepage".to_owned()));
    state
        .settings
        .entry(NEW_TAB_CUSTOM_URL_SETTING_KEY.to_owned())
        .or_insert_with(|| SettingValue::Text("https://example.com".to_owned()));
    state
        .settings
        .entry(KEYBINDING_CLOSE_TAB_SETTING_KEY.to_owned())
        .or_insert_with(|| SettingValue::Text("mod+w".to_owned()));
    state
        .settings
        .entry(KEYBINDING_COMMAND_PALETTE_SETTING_KEY.to_owned())
        .or_insert_with(|| SettingValue::Text("space".to_owned()));
    state
        .settings
        .entry(KEYBINDING_FOCUS_NAVIGATION_SETTING_KEY.to_owned())
        .or_insert_with(|| SettingValue::Text("mod+l".to_owned()));
    state
        .settings
        .entry(KEYBINDING_TOGGLE_DEVTOOLS_SETTING_KEY.to_owned())
        .or_insert_with(|| SettingValue::Text("mod+shift+i".to_owned()));
    state
        .settings
        .entry(PASSWORD_MANAGER_DEFAULT_PROVIDER_SETTING_KEY.to_owned())
        .or_insert_with(|| SettingValue::Text("builtin".to_owned()));
    state
        .settings
        .entry(PASSWORD_MANAGER_DEFAULT_AUTOFILL_SETTING_KEY.to_owned())
        .or_insert_with(|| SettingValue::Text("enabled".to_owned()));
    state
        .settings
        .entry(PASSWORD_MANAGER_DEFAULT_SAVE_PROMPT_SETTING_KEY.to_owned())
        .or_insert_with(|| SettingValue::Text("enabled".to_owned()));
    state
        .settings
        .entry(PASSWORD_MANAGER_DEFAULT_FALLBACK_SETTING_KEY.to_owned())
        .or_insert_with(|| SettingValue::Text("builtin".to_owned()));
}

fn ensure_bootstrap_state(state: &mut BrowserState) -> WorkspaceId {
    if state.profiles.is_empty() {
        let profile_id = state.add_profile("Default");
        let workspace_id = state
            .add_workspace(profile_id, "Workspace 1")
            .expect("bootstrap profile must exist");
        ensure_default_settings(state);
        state.recompute_next_ids();
        return workspace_id;
    }

    ensure_default_settings(state);
    state.recompute_next_ids();

    if state
        .active_profile_id
        .map(|profile_id| !state.profiles.contains_key(&profile_id))
        .unwrap_or(true)
    {
        state.active_profile_id = state.profiles.keys().next().copied();
    }

    let profile_ids: Vec<ProfileId> = state.profiles.keys().copied().collect();
    for profile_id in profile_ids {
        let mut workspace_order = state
            .workspaces
            .iter()
            .filter(|(_, workspace)| workspace.profile_id == profile_id)
            .map(|(workspace_id, _)| *workspace_id)
            .collect::<Vec<_>>();
        workspace_order.sort();

        if let Some(profile) = state.profiles.get_mut(&profile_id) {
            if profile.workspace_order.is_empty() {
                profile.workspace_order = workspace_order;
            }
            if profile
                .active_workspace_id
                .map(|workspace_id| !profile.workspace_order.contains(&workspace_id))
                .unwrap_or(true)
            {
                profile.active_workspace_id = profile.workspace_order.first().copied();
            }
        }
    }

    let workspace_ids: Vec<WorkspaceId> = state.workspaces.keys().copied().collect();
    for workspace_id in workspace_ids {
        let mut tab_order = state
            .tabs
            .iter()
            .filter(|(_, tab)| tab.workspace_id == workspace_id)
            .map(|(tab_id, _)| *tab_id)
            .collect::<Vec<_>>();
        tab_order.sort();

        if let Some(workspace) = state.workspaces.get_mut(&workspace_id) {
            if workspace.tab_order.is_empty() {
                workspace.tab_order = tab_order;
            }
            if workspace
                .active_tab_id
                .map(|tab_id| !workspace.tab_order.contains(&tab_id))
                .unwrap_or(true)
            {
                workspace.active_tab_id = workspace.tab_order.first().copied();
            }

            if let Some(active_tab_id) = workspace.active_tab_id {
                if let Some(active_tab) = state.tabs.get_mut(&active_tab_id) {
                    active_tab.runtime_state = TabRuntimeState::Active;
                }
            }
        }
    }

    if let Some(active_profile_id) = state.active_profile_id {
        if let Some(workspace_id) = state
            .profiles
            .get(&active_profile_id)
            .and_then(|profile| profile.active_workspace_id)
        {
            return workspace_id;
        }
    }

    if let Some(workspace_id) = state.workspaces.keys().next().copied() {
        let profile_id = state
            .workspaces
            .get(&workspace_id)
            .map(|workspace| workspace.profile_id)
            .expect("workspace id came from map key");
        state.active_profile_id = Some(profile_id);
        if let Some(profile) = state.profiles.get_mut(&profile_id) {
            if !profile.workspace_order.contains(&workspace_id) {
                profile.workspace_order.insert(0, workspace_id);
            }
            profile.active_workspace_id = Some(workspace_id);
        }
        return workspace_id;
    }

    let profile_id = state
        .active_profile_id
        .unwrap_or_else(|| state.add_profile("Default"));
    let workspace_id = state
        .add_workspace(profile_id, "Workspace 1")
        .expect("profile must exist");
    state.active_profile_id = Some(profile_id);
    ensure_default_settings(state);
    state.recompute_next_ids();
    workspace_id
}

impl<HError: Display> Display for RuntimeError<HError> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::PersistenceInit(message) => {
                write!(f, "persistence initialization failed: {message}")
            }
            Self::Host(err) => write!(f, "host error: {err}"),
            Self::Engine(err) => write!(f, "engine error: {err:?}"),
            Self::NoActiveWorkspace => {
                write!(f, "no active workspace available for UI navigation")
            }
            Self::NoActiveProfile => write!(f, "no active profile available for UI command"),
            Self::BlockedContentNavigation(url) => {
                write!(f, "content navigation blocked for url: {url}")
            }
        }
    }
}

impl<HError: Error + 'static> Error for RuntimeError<HError> {}

fn build_thumbnail_data_url(title: &str, url: &str) -> String {
    let title_line = if title.trim().is_empty() {
        "Untitled Tab"
    } else {
        title.trim()
    };
    let subtitle = if url.trim().is_empty() {
        "about:blank"
    } else {
        url.trim()
    };
    let title_line = escape_xml(title_line);
    let subtitle = escape_xml(subtitle);
    let svg = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='288' height='180' viewBox='0 0 288 180'><defs><linearGradient id='bg' x1='0' y1='0' x2='1' y2='1'><stop offset='0%' stop-color='#111b31'/><stop offset='100%' stop-color='#1f365f'/></linearGradient></defs><rect width='288' height='180' fill='url(#bg)'/><rect x='12' y='12' width='264' height='156' rx='10' fill='rgba(8,16,30,0.62)' stroke='rgba(126,164,255,0.35)'/><text x='20' y='74' fill='#e7efff' font-size='15' font-family='-apple-system, Segoe UI, sans-serif'>{title_line}</text><text x='20' y='101' fill='#9fb5e3' font-size='11' font-family='-apple-system, Segoe UI, sans-serif'>{subtitle}</text></svg>"
    );
    format!(
        "data:image/svg+xml;utf8,{}",
        percent_encode_uri_component(&svg)
    )
}

fn percent_encode_uri_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len() + 32);
    for byte in value.bytes() {
        let keep = byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~');
        if keep {
            encoded.push(byte as char);
        } else {
            encoded.push('%');
            encoded.push_str(&format!("{byte:02X}"));
        }
    }
    encoded
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\"', "&quot;")
        .replace('\'', "&apos;")
}

fn push_json_string(json: &mut String, value: &str) {
    json.push('"');
    for ch in value.chars() {
        match ch {
            '"' => json.push_str("\\\""),
            '\\' => json.push_str("\\\\"),
            '\n' => json.push_str("\\n"),
            '\r' => json.push_str("\\r"),
            '\t' => json.push_str("\\t"),
            '\u{08}' => json.push_str("\\b"),
            '\u{0c}' => json.push_str("\\f"),
            c if c <= '\u{1f}' => {
                let code = c as u32;
                json.push_str("\\u");
                let hex = format!("{code:04x}");
                json.push_str(&hex);
            }
            c => json.push(c),
        }
    }
    json.push('"');
}

fn push_json_setting_value(json: &mut String, value: &SettingValue) {
    match value {
        SettingValue::Bool(flag) => json.push_str(if *flag { "true" } else { "false" }),
        SettingValue::Int(number) => json.push_str(&number.to_string()),
        SettingValue::Text(text) => push_json_string(json, text),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::bridge::UiCommand;
    use crate::host::{
        CefHost, ContentEvent, ContentViewId, HostError, HostEvent, MockCefHost, UiViewId,
        WindowEvent, WindowId, WindowSize,
    };
    use switchboard_core::{Intent, SettingValue, TabId, TabRuntimeState};

    use super::{AppRuntime, RuntimeError};

    #[derive(Clone)]
    struct RecordingHost {
        next_window_id: u64,
        next_ui_view_id: u64,
        next_content_view_id: u64,
        events: Rc<RefCell<Vec<HostEvent>>>,
    }

    impl RecordingHost {
        fn new(events: Rc<RefCell<Vec<HostEvent>>>) -> Self {
            Self {
                next_window_id: 0,
                next_ui_view_id: 0,
                next_content_view_id: 0,
                events,
            }
        }
    }

    impl CefHost for RecordingHost {
        type Error = HostError;

        fn create_window(
            &mut self,
            title: &str,
            size: WindowSize,
        ) -> Result<WindowId, Self::Error> {
            self.next_window_id += 1;
            let window_id = WindowId(self.next_window_id);
            self.events.borrow_mut().push(HostEvent::WindowCreated {
                window_id,
                title: title.to_owned(),
                size,
            });
            Ok(window_id)
        }

        fn create_ui_view(
            &mut self,
            window_id: WindowId,
            url: &str,
        ) -> Result<UiViewId, Self::Error> {
            if !url.starts_with("app://ui") {
                return Err(HostError::InvalidUiUrl(url.to_owned()));
            }
            self.next_ui_view_id += 1;
            let view_id = UiViewId(self.next_ui_view_id);
            self.events.borrow_mut().push(HostEvent::UiViewCreated {
                window_id,
                view_id,
                url: url.to_owned(),
            });
            Ok(view_id)
        }

        fn create_content_view(
            &mut self,
            window_id: WindowId,
            tab_id: TabId,
            url: &str,
        ) -> Result<ContentViewId, Self::Error> {
            self.next_content_view_id += 1;
            let view_id = ContentViewId(self.next_content_view_id);
            self.events
                .borrow_mut()
                .push(HostEvent::ContentViewCreated {
                    window_id,
                    view_id,
                    tab_id,
                    url: url.to_owned(),
                });
            Ok(view_id)
        }

        fn navigate_content_view(
            &mut self,
            view_id: ContentViewId,
            tab_id: TabId,
            url: &str,
        ) -> Result<(), Self::Error> {
            self.events.borrow_mut().push(HostEvent::ContentNavigated {
                view_id,
                tab_id,
                url: url.to_owned(),
            });
            Ok(())
        }

        fn set_content_view_visible(
            &mut self,
            _view_id: ContentViewId,
            _visible: bool,
        ) -> Result<(), Self::Error> {
            Ok(())
        }

        fn toggle_dev_tools_for_active_content(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }

        fn clear_content_view(&mut self, _view_id: ContentViewId) -> Result<(), Self::Error> {
            Ok(())
        }

        fn destroy_content_view(&mut self, view_id: ContentViewId) -> Result<(), Self::Error> {
            self.events
                .borrow_mut()
                .push(HostEvent::ContentViewDestroyed { view_id });
            Ok(())
        }

        fn run_event_loop(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    #[test]
    fn bootstrap_creates_window_and_ui_shell_view() {
        let host = MockCefHost::default();
        let runtime = AppRuntime::bootstrap(host, "0.1.0").expect("bootstrap should succeed");

        assert_eq!(runtime.revision(), 0);
        assert_eq!(runtime.ui_view_id().0, 1);
        assert_eq!(runtime.host().events().len(), 2);
        assert!(matches!(
            &runtime.host().events()[0],
            HostEvent::WindowCreated { .. }
        ));
        assert!(matches!(
            &runtime.host().events()[1],
            HostEvent::UiViewCreated { url, .. } if url.starts_with("app://ui")
        ));
    }

    #[test]
    fn first_navigation_creates_content_view_then_reuses_it() {
        let host = MockCefHost::default();
        let mut runtime = AppRuntime::bootstrap(host, "0.1.0").expect("bootstrap should succeed");
        let workspace_id = runtime.default_workspace_id();

        runtime
            .handle_ui_command(UiCommand::NewTab {
                workspace_id: workspace_id.0,
                url: None,
                make_active: true,
            })
            .expect("new tab should succeed");
        let tab_id = runtime
            .active_tab_id(workspace_id)
            .expect("new tab should be active");

        runtime
            .handle_ui_command(UiCommand::Navigate {
                tab_id: tab_id.0,
                url: "https://example.com".to_owned(),
            })
            .expect("first navigation should create content view");
        runtime
            .handle_ui_command(UiCommand::Navigate {
                tab_id: tab_id.0,
                url: "https://rust-lang.org".to_owned(),
            })
            .expect("second navigation should reuse content view");

        let content_create_count = runtime
            .host()
            .events()
            .iter()
            .filter(|event| matches!(event, HostEvent::ContentViewCreated { .. }))
            .count();
        let content_navigate_count = runtime
            .host()
            .events()
            .iter()
            .filter(|event| matches!(event, HostEvent::ContentNavigated { .. }))
            .count();

        assert_eq!(content_create_count, 1);
        assert_eq!(content_navigate_count, 2);
    }

    #[test]
    fn blocks_content_navigation_to_app_scheme() {
        let host = MockCefHost::default();
        let mut runtime = AppRuntime::bootstrap(host, "0.1.0").expect("bootstrap should succeed");
        let workspace_id = runtime.default_workspace_id();

        runtime
            .handle_ui_command(UiCommand::NewTab {
                workspace_id: workspace_id.0,
                url: None,
                make_active: true,
            })
            .expect("new tab should succeed");
        let tab_id = runtime
            .active_tab_id(workspace_id)
            .expect("new tab should be active");

        let result = runtime.handle_ui_command(UiCommand::Navigate {
            tab_id: tab_id.0,
            url: "app://ui/settings".to_owned(),
        });

        assert!(matches!(
            result,
            Err(RuntimeError::BlockedContentNavigation(url)) if url == "app://ui/settings"
        ));
    }

    #[test]
    fn navigate_active_targets_current_active_tab() {
        let host = MockCefHost::default();
        let mut runtime = AppRuntime::bootstrap(host, "0.1.0").expect("bootstrap should succeed");
        let workspace_id = runtime.default_workspace_id();

        runtime
            .handle_ui_command(UiCommand::NewTab {
                workspace_id: workspace_id.0,
                url: None,
                make_active: true,
            })
            .expect("new tab should succeed");

        runtime
            .handle_ui_command(UiCommand::NavigateActive {
                url: "https://example.com".to_owned(),
            })
            .expect("navigate active should succeed");

        let content_create_count = runtime
            .host()
            .events()
            .iter()
            .filter(|event| matches!(event, HostEvent::ContentViewCreated { .. }))
            .count();

        assert_eq!(content_create_count, 1);
    }

    #[test]
    fn navigate_active_creates_tab_for_empty_workspace() {
        let host = MockCefHost::default();
        let mut runtime = AppRuntime::bootstrap(host, "0.1.0").expect("bootstrap should succeed");
        let initial_workspace_id = runtime.default_workspace_id();

        runtime
            .handle_ui_command(UiCommand::NewWorkspace {
                name: "Workspace 2".to_owned(),
            })
            .expect("new workspace should succeed");

        let second_workspace_id = runtime
            .engine()
            .state()
            .workspaces
            .values()
            .find(|workspace| workspace.id != initial_workspace_id)
            .map(|workspace| workspace.id)
            .expect("second workspace should exist");

        runtime
            .handle_ui_command(UiCommand::SwitchWorkspace {
                workspace_id: second_workspace_id.0,
            })
            .expect("switch workspace should succeed");
        runtime
            .handle_ui_command(UiCommand::NavigateActive {
                url: "https://example.com".to_owned(),
            })
            .expect("navigate active should create a tab");

        let active_tab_id = runtime
            .active_tab_id(second_workspace_id)
            .expect("new tab should be active in second workspace");
        let active_tab = runtime
            .engine()
            .state()
            .tabs
            .get(&active_tab_id)
            .expect("new tab should exist");
        assert_eq!(active_tab.url, "https://example.com");
    }

    #[test]
    fn activate_tab_updates_content_view_to_selected_tab() {
        let host = MockCefHost::default();
        let mut runtime = AppRuntime::bootstrap(host, "0.1.0").expect("bootstrap should succeed");
        let workspace_id = runtime.default_workspace_id();

        runtime
            .handle_ui_command(UiCommand::NewTab {
                workspace_id: workspace_id.0,
                url: Some("https://one.example".to_owned()),
                make_active: true,
            })
            .expect("first tab should succeed");
        let first_tab_id = runtime
            .active_tab_id(workspace_id)
            .expect("first tab should be active");

        runtime
            .handle_ui_command(UiCommand::NewTab {
                workspace_id: workspace_id.0,
                url: Some("https://two.example".to_owned()),
                make_active: true,
            })
            .expect("second tab should succeed");

        runtime
            .handle_ui_command(UiCommand::ActivateTab {
                tab_id: first_tab_id.0,
            })
            .expect("activate tab should succeed");

        let content_create_count = runtime
            .host()
            .events()
            .iter()
            .filter(|event| matches!(event, HostEvent::ContentViewCreated { .. }))
            .count();
        let content_navigate_count = runtime
            .host()
            .events()
            .iter()
            .filter(|event| matches!(event, HostEvent::ContentNavigated { .. }))
            .count();
        assert_eq!(content_create_count, 2);
        assert_eq!(content_navigate_count, 0);
    }

    #[test]
    fn activate_active_tab_is_noop_for_content_view() {
        let host = MockCefHost::default();
        let mut runtime = AppRuntime::bootstrap(host, "0.1.0").expect("bootstrap should succeed");
        let workspace_id = runtime.default_workspace_id();

        runtime
            .handle_ui_command(UiCommand::NewTab {
                workspace_id: workspace_id.0,
                url: Some("https://one.example".to_owned()),
                make_active: true,
            })
            .expect("tab should be created");
        let active_tab_id = runtime
            .active_tab_id(workspace_id)
            .expect("tab should be active");
        let revision_before = runtime.revision();

        let patch = runtime
            .handle_ui_command(UiCommand::ActivateTab {
                tab_id: active_tab_id.0,
            })
            .expect("activate active tab should succeed");

        assert!(
            patch.ops.is_empty(),
            "activate on already-active tab should no-op"
        );
        assert_eq!(runtime.revision(), revision_before);

        let content_create_count = runtime
            .host()
            .events()
            .iter()
            .filter(|event| matches!(event, HostEvent::ContentViewCreated { .. }))
            .count();
        let content_navigate_count = runtime
            .host()
            .events()
            .iter()
            .filter(|event| matches!(event, HostEvent::ContentNavigated { .. }))
            .count();
        assert_eq!(content_create_count, 1);
        assert_eq!(content_navigate_count, 0);
    }

    #[test]
    fn content_events_update_tab_metadata() {
        let host = MockCefHost::default();
        let mut runtime = AppRuntime::bootstrap(host, "0.1.0").expect("bootstrap should succeed");
        let workspace_id = runtime.default_workspace_id();

        runtime
            .handle_ui_command(UiCommand::NewTab {
                workspace_id: workspace_id.0,
                url: Some("https://one.example".to_owned()),
                make_active: true,
            })
            .expect("tab should be created");
        let tab_id = runtime
            .active_tab_id(workspace_id)
            .expect("tab should be active");

        runtime
            .handle_content_event(ContentEvent::TitleChanged {
                tab_id,
                title: "One Example".to_owned(),
            })
            .expect("title event should apply");
        runtime
            .handle_content_event(ContentEvent::LoadingChanged {
                tab_id,
                is_loading: true,
            })
            .expect("loading event should apply");

        let tab = runtime
            .engine()
            .state()
            .tabs
            .get(&tab_id)
            .expect("tab should exist");
        assert_eq!(tab.title, "One Example");
        assert!(tab.loading);
    }

    #[test]
    fn stale_content_events_are_ignored() {
        let host = MockCefHost::default();
        let mut runtime = AppRuntime::bootstrap(host, "0.1.0").expect("bootstrap should succeed");
        let workspace_id = runtime.default_workspace_id();

        runtime
            .handle_ui_command(UiCommand::NewTab {
                workspace_id: workspace_id.0,
                url: Some("https://one.example".to_owned()),
                make_active: true,
            })
            .expect("tab should be created");
        let tab_id = runtime
            .active_tab_id(workspace_id)
            .expect("tab should be active");

        runtime
            .handle_ui_command(UiCommand::CloseTab { tab_id: tab_id.0 })
            .expect("close tab should succeed");
        let revision_before = runtime.revision();

        let patch = runtime
            .handle_content_event(ContentEvent::TitleChanged {
                tab_id,
                title: "stale".to_owned(),
            })
            .expect("stale event should be ignored");

        assert!(patch.ops.is_empty());
        assert_eq!(patch.from_revision, revision_before);
        assert_eq!(patch.to_revision, revision_before);
    }

    #[test]
    fn run_hydrates_active_tab_when_bindings_are_missing() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let host = RecordingHost::new(events.clone());
        let mut runtime = AppRuntime::bootstrap(host, "0.1.0").expect("bootstrap should succeed");
        let workspace_id = runtime.default_workspace_id();

        runtime
            .handle_ui_command(UiCommand::NewTab {
                workspace_id: workspace_id.0,
                url: Some("https://restore.example".to_owned()),
                make_active: true,
            })
            .expect("tab creation should succeed");

        let initial_content_creates = events
            .borrow()
            .iter()
            .filter(|event| matches!(event, HostEvent::ContentViewCreated { .. }))
            .count();
        assert_eq!(initial_content_creates, 1);

        runtime.tab_bindings.clear();
        runtime.run().expect("run should succeed");

        let post_run_content_creates = events
            .borrow()
            .iter()
            .filter(|event| matches!(event, HostEvent::ContentViewCreated { .. }))
            .count();
        assert_eq!(post_run_content_creates, 2);
    }

    #[test]
    fn window_resize_event_persists_settings() {
        let host = MockCefHost::default();
        let mut runtime = AppRuntime::bootstrap(host, "0.1.0").expect("bootstrap should succeed");

        let patch = runtime
            .handle_window_event(WindowEvent::Resized {
                width: 1440,
                height: 900,
            })
            .expect("window resize should persist");
        assert!(
            !patch.ops.is_empty(),
            "first resize should emit settings patch ops"
        );

        let width = runtime
            .engine()
            .state()
            .settings
            .get("window.width")
            .expect("width setting should exist");
        let height = runtime
            .engine()
            .state()
            .settings
            .get("window.height")
            .expect("height setting should exist");
        assert_eq!(width, &switchboard_core::SettingValue::Int(1440));
        assert_eq!(height, &switchboard_core::SettingValue::Int(900));

        let revision_before = runtime.revision();
        let noop_patch = runtime
            .handle_window_event(WindowEvent::Resized {
                width: 1440,
                height: 900,
            })
            .expect("repeat resize should be a no-op");
        assert!(noop_patch.ops.is_empty());
        assert_eq!(noop_patch.from_revision, revision_before);
        assert_eq!(noop_patch.to_revision, revision_before);
    }

    #[test]
    fn patch_revisions_remain_monotonic_and_contiguous() {
        let host = MockCefHost::default();
        let mut runtime = AppRuntime::bootstrap(host, "0.1.0").expect("bootstrap should succeed");
        let workspace_id = runtime.default_workspace_id();
        let mut expected_revision = runtime.revision();

        let patch = runtime
            .handle_ui_command(UiCommand::NewTab {
                workspace_id: workspace_id.0,
                url: Some("https://revision-one.example".to_owned()),
                make_active: true,
            })
            .expect("first command should succeed");
        assert_eq!(patch.from_revision, expected_revision);
        assert_eq!(patch.to_revision, expected_revision + 1);
        expected_revision = patch.to_revision;

        let active_tab_id = runtime
            .active_tab_id(workspace_id)
            .expect("new tab should be active");
        let patch = runtime
            .handle_ui_command(UiCommand::Navigate {
                tab_id: active_tab_id.0,
                url: "https://revision-two.example".to_owned(),
            })
            .expect("navigation should succeed");
        assert_eq!(patch.from_revision, expected_revision);
        assert_eq!(patch.to_revision, expected_revision + 1);
        expected_revision = patch.to_revision;

        let patch = runtime
            .handle_ui_command(UiCommand::NewWorkspace {
                name: "Revision Workspace".to_owned(),
            })
            .expect("new workspace should succeed");
        assert_eq!(patch.from_revision, expected_revision);
        assert_eq!(patch.to_revision, expected_revision + 1);
        expected_revision = patch.to_revision;

        let patch = runtime
            .handle_ui_command(UiCommand::ActivateTab {
                tab_id: active_tab_id.0,
            })
            .expect("activate already-active tab should succeed");
        assert_eq!(patch.from_revision, expected_revision);
        assert_eq!(patch.to_revision, expected_revision);
        assert!(patch.ops.is_empty());
        assert_eq!(runtime.revision(), expected_revision);
    }

    #[test]
    fn shell_state_json_supports_full_resync_after_revision_drift() {
        let host = MockCefHost::default();
        let mut runtime = AppRuntime::bootstrap(host, "0.1.0").expect("bootstrap should succeed");
        let workspace_id = runtime.default_workspace_id();
        let stale_revision = runtime.revision();

        runtime
            .handle_ui_command(UiCommand::NewTab {
                workspace_id: workspace_id.0,
                url: Some("https://resync.example".to_owned()),
                make_active: true,
            })
            .expect("tab creation should succeed");
        let active_tab_id = runtime
            .active_tab_id(workspace_id)
            .expect("active tab should exist");
        runtime
            .handle_ui_command(UiCommand::Navigate {
                tab_id: active_tab_id.0,
                url: "https://resync.example/latest".to_owned(),
            })
            .expect("navigate should succeed");

        let latest_revision = runtime.revision();
        assert!(latest_revision > stale_revision);
        let state_json = runtime.ui_shell_state_json();
        assert!(state_json.contains(&format!("\"revision\":{latest_revision}")));
        assert!(state_json.contains("\"tabs\":["));
        assert!(state_json.contains("https://resync.example/latest"));
    }

    #[test]
    fn shell_state_json_exposes_settings_and_updates_after_setting_set() {
        let host = MockCefHost::default();
        let mut runtime = AppRuntime::bootstrap(host, "0.1.0").expect("bootstrap should succeed");

        let initial = runtime.ui_shell_state_json();
        assert!(initial.contains("\"settings\":{"));
        assert!(initial.contains("\"search_engine\":\"google\""));
        assert!(initial.contains("\"homepage\":\"https://youtube.com\""));
        assert!(initial.contains("\"new_tab_behavior\":\"homepage\""));
        assert!(initial.contains("\"keybinding_close_tab\":\"mod+w\""));
        assert!(initial.contains("\"keybinding_command_palette\":\"space\""));
        assert!(initial.contains("\"keybinding_focus_navigation\":\"mod+l\""));
        assert!(initial.contains("\"keybinding_toggle_devtools\":\"mod+shift+i\""));
        assert!(initial.contains("\"password_manager.default_provider\":\"builtin\""));
        assert!(initial.contains("\"password_manager.default_autofill\":\"enabled\""));
        assert!(initial.contains("\"password_manager.default_save_prompt\":\"enabled\""));
        assert!(initial.contains("\"password_manager.default_fallback\":\"builtin\""));

        runtime
            .handle_ui_command(UiCommand::SettingSet {
                key: "search_engine".to_owned(),
                value: SettingValue::Text("duckduckgo".to_owned()),
            })
            .expect("setting update should succeed");

        let updated = runtime.ui_shell_state_json();
        assert!(updated.contains("\"search_engine\":\"duckduckgo\""));
    }

    #[test]
    fn toggle_devtools_is_state_noop() {
        let host = MockCefHost::default();
        let mut runtime = AppRuntime::bootstrap(host, "0.1.0").expect("bootstrap should succeed");
        let revision = runtime.revision();

        let patch = runtime
            .handle_ui_command(UiCommand::ToggleDevTools)
            .expect("devtools toggle should succeed");
        assert!(patch.ops.is_empty());
        assert_eq!(patch.from_revision, revision);
        assert_eq!(patch.to_revision, revision);
    }

    #[test]
    fn lifecycle_policy_drives_live_view_set_under_runtime_churn() {
        let host = MockCefHost::default();
        let mut runtime = AppRuntime::bootstrap(host, "0.1.0").expect("bootstrap should succeed");
        let default_workspace_id = runtime.default_workspace_id();

        runtime
            .handle_intent(Intent::SettingSet {
                key: "warm_pool_budget".to_owned(),
                value: SettingValue::Int(2),
            })
            .expect("warm pool budget update should succeed");

        runtime
            .handle_ui_command(UiCommand::NewProfile {
                name: "Work".to_owned(),
            })
            .expect("second profile should be created");
        let second_profile_id = runtime
            .engine()
            .state()
            .profiles
            .keys()
            .copied()
            .find(|profile_id| profile_id.0 != 1)
            .expect("second profile id should exist");
        let second_workspace_id = runtime
            .engine()
            .state()
            .profiles
            .get(&second_profile_id)
            .and_then(|profile| profile.active_workspace_id)
            .expect("second profile should have active workspace");

        runtime
            .handle_ui_command(UiCommand::SwitchProfile { profile_id: 1 })
            .expect("switching back to first profile should succeed");

        for i in 0..24u64 {
            let target_workspace_id = if i % 2 == 0 {
                default_workspace_id
            } else {
                second_workspace_id
            };
            let target_profile_id = if i % 2 == 0 { 1 } else { second_profile_id.0 };
            runtime
                .handle_ui_command(UiCommand::SwitchProfile {
                    profile_id: target_profile_id,
                })
                .expect("profile switch should succeed");
            runtime
                .handle_ui_command(UiCommand::SwitchWorkspace {
                    workspace_id: target_workspace_id.0,
                })
                .expect("workspace switch should succeed");
            runtime
                .handle_ui_command(UiCommand::NewTab {
                    workspace_id: target_workspace_id.0,
                    url: Some(format!("https://lifecycle-{i}.example")),
                    make_active: true,
                })
                .expect("tab creation should succeed");

            let live_state_count = runtime
                .engine()
                .state()
                .tabs
                .values()
                .filter(|tab| {
                    matches!(
                        tab.runtime_state,
                        TabRuntimeState::Active | TabRuntimeState::Warm
                    )
                })
                .count();
            assert_eq!(runtime.tab_bindings.len(), live_state_count);

            let active_tab_id = runtime.resolve_active_tab_id();
            if let Some(active_tab_id) = active_tab_id {
                assert!(
                    runtime.tab_bindings.contains_key(&active_tab_id),
                    "active tab must have a live content view binding"
                );
            }
        }

        let destroy_events = runtime
            .host()
            .events()
            .iter()
            .filter(|event| matches!(event, HostEvent::ContentViewDestroyed { .. }))
            .count();
        assert!(destroy_events > 0, "discarded tabs should destroy views");
    }

    #[test]
    fn switching_profiles_uses_separate_content_views() {
        let host = MockCefHost::default();
        let mut runtime = AppRuntime::bootstrap(host, "0.1.0").expect("bootstrap should succeed");
        let default_profile_id = runtime
            .engine()
            .state()
            .active_profile_id
            .expect("default profile should exist");
        let default_workspace_id = runtime.default_workspace_id();

        runtime
            .handle_ui_command(UiCommand::NewTab {
                workspace_id: default_workspace_id.0,
                url: Some("https://one.example".to_owned()),
                make_active: true,
            })
            .expect("default profile tab should be created");

        runtime
            .handle_ui_command(UiCommand::NewProfile {
                name: "Work".to_owned(),
            })
            .expect("new profile should be created");
        let second_profile_id = runtime
            .engine()
            .state()
            .profiles
            .keys()
            .copied()
            .find(|id| *id != default_profile_id)
            .expect("second profile should exist");
        let second_workspace_id = runtime
            .engine()
            .state()
            .profiles
            .get(&second_profile_id)
            .and_then(|profile| profile.active_workspace_id)
            .expect("second profile should have active workspace");

        runtime
            .handle_ui_command(UiCommand::NewTab {
                workspace_id: second_workspace_id.0,
                url: Some("https://two.example".to_owned()),
                make_active: true,
            })
            .expect("second profile tab should be created");

        runtime
            .handle_ui_command(UiCommand::SwitchProfile {
                profile_id: default_profile_id.0,
            })
            .expect("switching back to default profile should succeed");

        let content_create_count = runtime
            .host()
            .events()
            .iter()
            .filter(|event| matches!(event, HostEvent::ContentViewCreated { .. }))
            .count();
        let content_destroy_count = runtime
            .host()
            .events()
            .iter()
            .filter(|event| matches!(event, HostEvent::ContentViewDestroyed { .. }))
            .count();
        assert!(content_create_count >= 2);
        assert!(content_destroy_count >= 1);
    }

    #[test]
    fn loading_complete_captures_thumbnail_placeholder() {
        let host = MockCefHost::default();
        let mut runtime = AppRuntime::bootstrap(host, "0.1.0").expect("bootstrap should succeed");
        let workspace_id = runtime.default_workspace_id();

        runtime
            .handle_ui_command(UiCommand::NewTab {
                workspace_id: workspace_id.0,
                url: Some("https://thumbnail.example".to_owned()),
                make_active: true,
            })
            .expect("tab should be created");
        let tab_id = runtime
            .active_tab_id(workspace_id)
            .expect("tab should be active");

        runtime
            .handle_content_event(ContentEvent::LoadingChanged {
                tab_id,
                is_loading: false,
            })
            .expect("loading complete should update metadata");

        let tab = runtime
            .engine()
            .state()
            .tabs
            .get(&tab_id)
            .expect("tab should exist");
        let data_url = tab
            .thumbnail_data_url
            .as_ref()
            .expect("thumbnail placeholder should be captured");
        assert!(data_url.starts_with("data:image/svg+xml;utf8,"));
    }
}
