use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(test)]
use std::convert::Infallible;
#[cfg(test)]
use switchboard_core::NoopPersistence;
use switchboard_core::{
    BrowserState, Engine, EngineError, Intent, Patch, ProfileId, SettingValue, TabId,
    TabRuntimeState, TabStatus, WorkspaceId,
};

use crate::bridge::{
    encode_server_message, SearchResult, SearchResultKind, ServerEnvelope, ServerMessage,
    UiCommand, UiSnapshot, BRIDGE_PROTOCOL_VERSION, MAX_SEARCH_RESULTS,
};
use crate::host::{
    install_content_event_handler, install_ui_command_handler, install_window_event_handler,
    CefHost, ContentEvent, ContentViewId, UiViewId, WindowEvent, WindowId, WindowSize,
};
#[cfg(not(test))]
use crate::persistence::{AppPersistence, AppPersistenceError};

const UI_SHELL_URL_BASE: &str = "app://ui";
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
    generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LiveTabBinding {
    content: ContentBinding,
    last_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingRestore {
    profile_id: ProfileId,
    generation: u64,
    url: String,
}

pub struct AppRuntime<H: CefHost> {
    engine: Engine<RuntimePersistence>,
    host: H,
    window_id: WindowId,
    ui_view_id: UiViewId,
    default_workspace_id: WorkspaceId,
    tab_bindings: BTreeMap<TabId, LiveTabBinding>,
    pending_restores: BTreeMap<TabId, PendingRestore>,
    next_browser_generation: u64,
}

impl<H: CefHost + 'static> AppRuntime<H> {
    pub fn bootstrap(mut host: H, ui_version: &str) -> Result<Self, RuntimeError<H::Error>> {
        #[cfg(test)]
        let (persistence, mut state, revision) = (NoopPersistence, BrowserState::default(), 0);

        #[cfg(not(test))]
        let (persistence, mut state, revision) = {
            let persistence = AppPersistence::open_default()
                .map_err(|error| RuntimeError::PersistenceInit(error.to_string()))?;
            let loaded = persistence
                .load()
                .map_err(|error| RuntimeError::PersistenceInit(error.to_string()))?
                .unwrap_or_else(|| (BrowserState::default(), 0));
            (persistence, loaded.0, loaded.1)
        };

        let workspace_id = ensure_bootstrap_state(&mut state);
        let initial_window_size = restored_window_size(&state);
        let mut engine = Engine::with_state(persistence, state, revision);

        let window_id = host
            .create_window("Switchboard", initial_window_size)
            .map_err(RuntimeError::Host)?;
        let ui_shell_url = format!("{UI_SHELL_URL_BASE}?v={}", std::process::id());
        let ui_view_id = host
            .create_ui_view(window_id, &ui_shell_url)
            .map_err(RuntimeError::Host)?;

        let ui_ready = UiCommand::UiReady {
            ui_version: ui_version.to_owned(),
            last_revision: None,
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
            pending_restores: BTreeMap::new(),
            next_browser_generation: 1,
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

    #[cfg(test)]
    pub fn host(&self) -> &H {
        &self.host
    }

    pub fn run(mut self) -> Result<(), RuntimeError<H::Error>>
    where
        H::Error: Display,
    {
        let runtime_ptr: *mut Self = &mut self;
        install_ui_command_handler(Some(Box::new(move |command| unsafe {
            if let Err(error) = (*runtime_ptr).handle_ui_command(command) {
                eprintln!("switchboard-app: UI command failed: {error}");
            }
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
        install_content_event_handler(None);
        install_window_event_handler(None);
        result
    }

    pub fn handle_ui_command(
        &mut self,
        command: UiCommand,
    ) -> Result<Patch, RuntimeError<H::Error>> {
        match command {
            UiCommand::UiReady { .. } => {
                let _ = self.handle_intent(Intent::WakeSnoozed { now_ms: now_ms() })?;
                self.send_snapshot(None)?;
                self.sync_runtime_views()?;
                Ok(self.empty_patch())
            }
            UiCommand::RequestResync { .. } => {
                self.send_snapshot(None)?;
                self.sync_runtime_views()?;
                Ok(self.empty_patch())
            }
            UiCommand::FrameCommitted { tab_id, generation } => {
                self.commit_restored_frame(TabId(tab_id), generation)
            }
            UiCommand::Search { query, limit } => {
                let results = self.search(&query, limit.unwrap_or(20).min(MAX_SEARCH_RESULTS));
                self.send_server_message(None, ServerMessage::SearchResults { query, results })?;
                Ok(self.empty_patch())
            }
            UiCommand::SetUiOverlay { .. }
            | UiCommand::SetBrowserMode { .. }
            | UiCommand::SyncKeybindings { .. } => Ok(self.empty_patch()),
            UiCommand::SetFocusMode { active } => {
                self.host
                    .set_focus_mode(active)
                    .map_err(RuntimeError::Host)?;
                self.sync_runtime_views()?;
                Ok(self.empty_patch())
            }
            UiCommand::OpenLinkFromTab {
                source_tab_id,
                generation,
                url,
                secondary,
            } => {
                let source_tab_id = TabId(source_tab_id);
                let current = self
                    .tab_bindings
                    .get(&source_tab_id)
                    .is_some_and(|binding| binding.content.generation == generation);
                if !current {
                    return Ok(self.empty_patch());
                }
                self.handle_intent(Intent::OpenLinkFromTab {
                    source_tab_id,
                    url,
                    secondary,
                })
            }
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
            UiCommand::GoBack => {
                if let Some(binding) = self.active_binding() {
                    self.host
                        .go_back(binding.view_id)
                        .map_err(RuntimeError::Host)?;
                }
                Ok(self.empty_patch())
            }
            UiCommand::GoForward => {
                if let Some(binding) = self.active_binding() {
                    self.host
                        .go_forward(binding.view_id)
                        .map_err(RuntimeError::Host)?;
                }
                Ok(self.empty_patch())
            }
            UiCommand::Reload => {
                if let Some(binding) = self.active_binding() {
                    self.host
                        .reload(binding.view_id)
                        .map_err(RuntimeError::Host)?;
                }
                Ok(self.empty_patch())
            }
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
        if let Intent::Navigate { url, .. }
        | Intent::NewSubtab { url, .. }
        | Intent::OpenLinkFromTab { url, .. } = &intent
        {
            if url.starts_with("app://") {
                return Err(RuntimeError::BlockedContentNavigation(url.clone()));
            }
        }

        let patch = self.engine.dispatch(intent).map_err(RuntimeError::Engine)?;
        if !patch.ops.is_empty() {
            self.send_server_message(
                None,
                ServerMessage::Patch {
                    patch: patch.clone(),
                },
            )?;
        }
        self.sync_runtime_views()?;
        Ok(patch)
    }

    pub fn handle_content_event(
        &mut self,
        event: ContentEvent,
    ) -> Result<Patch, RuntimeError<H::Error>> {
        if let ContentEvent::BrowserClosed { tab_id, .. } = &event {
            if let Some(pending) = self.pending_restores.get(tab_id) {
                if !self.host.has_live_content_browser(*tab_id) {
                    self.send_server_message(
                        None,
                        ServerMessage::RestoreRequested {
                            tab_id: *tab_id,
                            generation: pending.generation,
                        },
                    )?;
                }
            }
            return Ok(self.empty_patch());
        }
        let (tab_id, generation) = match &event {
            ContentEvent::UrlChanged {
                tab_id, generation, ..
            }
            | ContentEvent::TitleChanged {
                tab_id, generation, ..
            }
            | ContentEvent::LoadingChanged {
                tab_id, generation, ..
            }
            | ContentEvent::MainFrameLoadSucceeded { tab_id, generation }
            | ContentEvent::NavigationStateChanged {
                tab_id, generation, ..
            } => (*tab_id, *generation),
            ContentEvent::BrowserClosed { .. } => unreachable!(),
        };

        let binding_is_current = self
            .tab_bindings
            .get(&tab_id)
            .is_some_and(|binding| binding.content.generation == generation);
        if !binding_is_current || !self.engine.state().tabs.contains_key(&tab_id) {
            return Ok(self.empty_patch());
        }

        if matches!(&event, ContentEvent::MainFrameLoadSucceeded { .. }) {
            if let Some(tab) = self.engine.state().tabs.get(&tab_id).cloned() {
                if tab.status.is_open()
                    && (tab.url.starts_with("https://") || tab.url.starts_with("http://"))
                {
                    return self.handle_intent(Intent::RecordHistory {
                        profile_id: tab.profile_id,
                        url: tab.url,
                        title: tab.title,
                        visited_at_ms: now_ms(),
                    });
                }
            }
            return Ok(self.empty_patch());
        }

        let intent = match event {
            ContentEvent::UrlChanged { url, .. } => Intent::ObserveTabUrl { tab_id, url },
            ContentEvent::TitleChanged { title, .. } => Intent::ObserveTabTitle { tab_id, title },
            ContentEvent::LoadingChanged { is_loading, .. } => {
                Intent::ObserveTabLoading { tab_id, is_loading }
            }
            ContentEvent::NavigationStateChanged {
                can_go_back,
                can_go_forward,
                ..
            } => Intent::ObserveTabNavigationState {
                tab_id,
                can_go_back,
                can_go_forward,
            },
            ContentEvent::MainFrameLoadSucceeded { .. } => unreachable!(),
            ContentEvent::BrowserClosed { .. } => unreachable!(),
        };

        self.handle_intent(intent)
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

    fn active_binding(&self) -> Option<ContentBinding> {
        let tab_id = self.resolve_active_tab_id()?;
        self.tab_bindings
            .get(&tab_id)
            .map(|binding| binding.content)
    }

    fn empty_patch(&self) -> Patch {
        Patch {
            ops: Vec::new(),
            from_revision: self.revision(),
            to_revision: self.revision(),
        }
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
        let active_workspace = self
            .resolve_active_workspace_id()
            .and_then(|id| self.engine.state().workspaces.get(&id).cloned());
        let primary_tab_id = active_workspace
            .as_ref()
            .and_then(|workspace| workspace.primary_tab_id);
        let secondary_tab_id = active_workspace
            .as_ref()
            .filter(|workspace| workspace.split_enabled)
            .and_then(|workspace| workspace.secondary_tab_id);

        let desired_live_tabs: Vec<(TabId, ProfileId, String)> = self
            .engine
            .state()
            .tabs
            .values()
            .filter(|tab| {
                matches!(
                    tab.runtime_state,
                    TabRuntimeState::Active
                        | TabRuntimeState::Secondary
                        | TabRuntimeState::Warm
                        | TabRuntimeState::Restoring
                )
            })
            .map(|tab| (tab.id, tab.profile_id, tab.url.clone()))
            .collect();
        let desired_live_ids: BTreeSet<TabId> = desired_live_tabs
            .iter()
            .map(|(tab_id, _, _)| *tab_id)
            .collect();
        self.pending_restores
            .retain(|tab_id, _| desired_live_ids.contains(tab_id));

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

        let mut restore_requests = Vec::new();
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
                    let pending = self.pending_restores.entry(tab_id).or_insert_with(|| {
                        let generation = self.next_browser_generation;
                        self.next_browser_generation =
                            self.next_browser_generation.saturating_add(1);
                        PendingRestore {
                            profile_id,
                            generation,
                            url: url.clone(),
                        }
                    });
                    if pending.profile_id != profile_id || pending.url != url {
                        pending.profile_id = profile_id;
                        pending.url = url;
                        pending.generation = self.next_browser_generation;
                        self.next_browser_generation =
                            self.next_browser_generation.saturating_add(1);
                    }
                    restore_requests.push((tab_id, pending.generation));
                }
            }
        }

        for (tab_id, generation) in restore_requests {
            self.send_server_message(None, ServerMessage::RestoreRequested { tab_id, generation })?;
        }

        let visibility: Vec<(TabId, ContentBinding)> = self
            .tab_bindings
            .iter()
            .map(|(tab_id, binding)| (*tab_id, binding.content))
            .collect();
        for (tab_id, binding) in visibility {
            let visible = (Some(tab_id) == primary_tab_id || Some(tab_id) == secondary_tab_id)
                && Some(binding.profile_id) == active_profile_id;
            self.host
                .set_content_view_visible(binding.view_id, visible)
                .map_err(RuntimeError::Host)?;
        }

        let primary_view = primary_tab_id
            .and_then(|tab_id| self.tab_bindings.get(&tab_id))
            .map(|binding| binding.content.view_id);
        let secondary_view = secondary_tab_id
            .and_then(|tab_id| self.tab_bindings.get(&tab_id))
            .map(|binding| binding.content.view_id);
        let ratio = active_workspace
            .as_ref()
            .map(|workspace| workspace.split_ratio)
            .unwrap_or(0.5);
        let split_enabled = active_workspace.as_ref().is_some_and(|workspace| {
            workspace.split_enabled && workspace.secondary_tab_id.is_some()
        });
        self.host
            .layout_content_views(primary_view, secondary_view, ratio, split_enabled)
            .map_err(RuntimeError::Host)?;

        Ok(())
    }

    fn commit_restored_frame(
        &mut self,
        tab_id: TabId,
        generation: u64,
    ) -> Result<Patch, RuntimeError<H::Error>> {
        let Some(pending) = self.pending_restores.get(&tab_id).cloned() else {
            return Ok(self.empty_patch());
        };
        if pending.generation != generation
            || !self.engine.state().tabs.get(&tab_id).is_some_and(|tab| {
                tab.profile_id == pending.profile_id
                    && tab.url == pending.url
                    && matches!(
                        tab.runtime_state,
                        TabRuntimeState::Active
                            | TabRuntimeState::Secondary
                            | TabRuntimeState::Warm
                            | TabRuntimeState::Restoring
                    )
            })
        {
            return Ok(self.empty_patch());
        }
        if self.host.has_live_content_browser(tab_id) {
            return Ok(self.empty_patch());
        }
        self.pending_restores.remove(&tab_id);
        let view_id = self
            .host
            .create_content_view(
                self.window_id,
                pending.profile_id,
                tab_id,
                generation,
                &pending.url,
            )
            .map_err(RuntimeError::Host)?;
        self.tab_bindings.insert(
            tab_id,
            LiveTabBinding {
                content: ContentBinding {
                    view_id,
                    profile_id: pending.profile_id,
                    generation,
                },
                last_url: pending.url,
            },
        );
        let patch = self
            .engine
            .dispatch(Intent::TabFrameCommitted { tab_id, generation })
            .map_err(RuntimeError::Engine)?;
        if !patch.ops.is_empty() {
            self.send_server_message(None, ServerMessage::Patch { patch })?;
        }
        self.sync_runtime_views()?;
        Ok(self.empty_patch())
    }

    pub fn ui_shell_state_json(&self) -> String {
        serde_json::to_string(&self.ui_snapshot()).unwrap_or_else(|error| {
            format!(
                "{{\"revision\":{},\"error\":{}}}",
                self.revision(),
                serde_json::to_string(&error.to_string()).unwrap_or_default()
            )
        })
    }

    fn ui_snapshot(&self) -> UiSnapshot {
        let state = self.engine.state();
        UiSnapshot {
            revision: self.revision(),
            active_profile_id: state.active_profile_id,
            profiles: state.profiles.values().cloned().collect(),
            workspaces: state.workspaces.values().cloned().collect(),
            tabs: state.tabs.values().cloned().collect(),
            settings: state.settings.clone(),
        }
    }

    fn send_snapshot(&mut self, request_id: Option<String>) -> Result<(), RuntimeError<H::Error>> {
        self.send_server_message(
            request_id,
            ServerMessage::Snapshot {
                snapshot: self.ui_snapshot(),
            },
        )
    }

    fn send_server_message(
        &mut self,
        request_id: Option<String>,
        message: ServerMessage,
    ) -> Result<(), RuntimeError<H::Error>> {
        let envelope = ServerEnvelope {
            protocol_version: BRIDGE_PROTOCOL_VERSION,
            request_id,
            revision: self.revision(),
            message,
        };
        let payload = encode_server_message(&envelope).map_err(|error| {
            RuntimeError::PersistenceInit(format!("bridge serialization failed: {error}"))
        })?;
        self.host
            .send_ui_message(&payload)
            .map_err(RuntimeError::Host)
    }

    fn search(&self, raw_query: &str, limit: usize) -> Vec<SearchResult> {
        let state = self.engine.state();
        let trimmed = raw_query.trim();
        let (mode, query) = if let Some(value) = trimmed.strip_prefix("done:") {
            ("done", value.trim())
        } else if let Some(value) = trimmed.strip_prefix("history:") {
            ("history", value.trim())
        } else if let Some(value) = trimmed.strip_prefix('>') {
            ("commands", value.trim())
        } else {
            ("all", trimmed)
        };
        let needle = query.to_ascii_lowercase();
        let mut ranked: Vec<(u8, u64, i64, SearchResult)> = Vec::new();

        if mode == "all" || mode == "commands" {
            for (id, title, subtitle) in [
                (
                    "new_workspace",
                    "New workspace",
                    "Create a workspace in the active profile",
                ),
                (
                    "show_archive",
                    "Open archive",
                    "Restore or permanently delete completed tabs",
                ),
                (
                    "toggle_split",
                    "Toggle split view",
                    "Show two tabs side by side",
                ),
                (
                    "toggle_focus",
                    "Toggle focus mode",
                    "Hide or show Switchboard chrome",
                ),
                (
                    "open_settings",
                    "Open settings",
                    "Configure browser behavior and shortcuts",
                ),
                (
                    "clear_profile_history",
                    "Clear profile history",
                    "Remove history for the active profile",
                ),
                (
                    "clear_archive",
                    "Clear completed tabs",
                    "Permanently delete unlocked completed tabs in this profile",
                ),
            ] {
                if let Some(score) = match_score(&needle, title, subtitle) {
                    ranked.push((
                        score,
                        0,
                        0,
                        SearchResult {
                            id: format!("command:{id}"),
                            kind: SearchResultKind::Command,
                            title: title.into(),
                            subtitle: subtitle.into(),
                            url: None,
                            profile_id: None,
                            workspace_id: None,
                            tab_id: None,
                            command: Some(id.into()),
                        },
                    ));
                }
            }
        }

        if mode == "all" {
            for tab in state.tabs.values().filter(|tab| tab.status.is_open()) {
                if let Some(score) = match_score(&needle, &tab.title, &tab.url) {
                    let profile = state
                        .profiles
                        .get(&tab.profile_id)
                        .map(|value| value.name.as_str())
                        .unwrap_or("Profile");
                    let workspace = state
                        .workspaces
                        .get(&tab.workspace_id)
                        .map(|value| value.name.as_str())
                        .unwrap_or("Workspace");
                    ranked.push((
                        score.saturating_add(1),
                        0,
                        0,
                        SearchResult {
                            id: format!("tab:{}", tab.id.0),
                            kind: SearchResultKind::OpenTab,
                            title: display_title(&tab.title, &tab.url),
                            subtitle: format!("{profile} · {workspace} · {}", tab.url),
                            url: Some(tab.url.clone()),
                            profile_id: Some(tab.profile_id),
                            workspace_id: Some(tab.workspace_id),
                            tab_id: Some(tab.id),
                            command: None,
                        },
                    ));
                }
            }
        }

        if mode == "done" {
            for tab in state
                .tabs
                .values()
                .filter(|tab| matches!(tab.status, TabStatus::Done { .. }))
            {
                if let Some(score) = match_score(&needle, &tab.title, &tab.url) {
                    let completed = match tab.status {
                        TabStatus::Done {
                            completed_at_ms, ..
                        } => completed_at_ms,
                        _ => 0,
                    };
                    ranked.push((
                        score,
                        0,
                        completed,
                        SearchResult {
                            id: format!("done:{}", tab.id.0),
                            kind: SearchResultKind::ArchivedTab,
                            title: display_title(&tab.title, &tab.url),
                            subtitle: tab.url.clone(),
                            url: Some(tab.url.clone()),
                            profile_id: Some(tab.profile_id),
                            workspace_id: Some(tab.workspace_id),
                            tab_id: Some(tab.id),
                            command: None,
                        },
                    ));
                }
            }
        }

        if mode == "all" || mode == "history" {
            if let Some(profile_id) = state.active_profile_id {
                if let Some(history) = state.history.get(&profile_id) {
                    for entry in history {
                        if let Some(score) = match_score(&needle, &entry.title, &entry.url) {
                            ranked.push((
                                score,
                                entry.visit_count,
                                entry.last_visit_ms,
                                SearchResult {
                                    id: format!("history:{}:{}", profile_id.0, entry.last_visit_ms),
                                    kind: SearchResultKind::History,
                                    title: display_title(&entry.title, &entry.url),
                                    subtitle: entry.url.clone(),
                                    url: Some(entry.url.clone()),
                                    profile_id: Some(profile_id),
                                    workspace_id: None,
                                    tab_id: None,
                                    command: None,
                                },
                            ));
                        }
                    }
                }
            }
        }

        if mode == "all" && !query.is_empty() {
            let url = web_fallback_url(state, query);
            ranked.push((
                0,
                0,
                0,
                SearchResult {
                    id: "web:fallback".into(),
                    kind: SearchResultKind::Web,
                    title: format!("Search the web for “{query}”"),
                    subtitle: url.clone(),
                    url: Some(url),
                    profile_id: state.active_profile_id,
                    workspace_id: state.active_workspace_id(),
                    tab_id: None,
                    command: None,
                },
            ));
        }

        ranked.sort_by(|left, right| {
            right
                .0
                .cmp(&left.0)
                .then_with(|| right.1.cmp(&left.1))
                .then_with(|| right.2.cmp(&left.2))
                .then_with(|| left.3.title.cmp(&right.3.title))
        });
        ranked
            .into_iter()
            .take(limit)
            .map(|(_, _, _, result)| result)
            .collect()
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

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

fn match_score(needle: &str, title: &str, subtitle: &str) -> Option<u8> {
    if needle.is_empty() {
        return Some(1);
    }
    let title = title.to_ascii_lowercase();
    let subtitle = subtitle.to_ascii_lowercase();
    if title == needle || subtitle == needle {
        Some(4)
    } else if title.starts_with(needle) || subtitle.starts_with(needle) {
        Some(3)
    } else if title.contains(needle) || subtitle.contains(needle) {
        Some(2)
    } else {
        None
    }
}

fn display_title(title: &str, url: &str) -> String {
    if title.trim().is_empty() {
        url.to_owned()
    } else {
        title.to_owned()
    }
}

fn web_fallback_url(state: &BrowserState, query: &str) -> String {
    let direct = query.trim();
    if direct.eq_ignore_ascii_case("about:blank")
        || direct.starts_with("https://")
        || direct.starts_with("http://")
    {
        return direct.to_owned();
    }
    if direct.contains('.') && !direct.contains(char::is_whitespace) {
        return format!("https://{direct}");
    }
    let encoded = percent_encode_query(direct);
    let engine = match state.settings.get(SEARCH_ENGINE_SETTING_KEY) {
        Some(SettingValue::Text(value)) => value.as_str(),
        _ => "google",
    };
    match engine {
        "duckduckgo" => format!("https://duckduckgo.com/?q={encoded}"),
        "bing" => format!("https://www.bing.com/search?q={encoded}"),
        _ => format!("https://www.google.com/search?q={encoded}"),
    }
}

fn percent_encode_query(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else if byte == b' ' {
            encoded.push('+');
        } else {
            encoded.push('%');
            encoded.push_str(&format!("{byte:02X}"));
        }
    }
    encoded
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
            .filter(|(_, tab)| tab.workspace_id == workspace_id && tab.status.is_open())
            .map(|(tab_id, _)| *tab_id)
            .collect::<Vec<_>>();
        tab_order.sort();

        if let Some(workspace) = state.workspaces.get_mut(&workspace_id) {
            if workspace.tab_order.is_empty() {
                workspace.tab_order = tab_order;
            }
            if workspace
                .primary_tab_id
                .map(|tab_id| !workspace.tab_order.contains(&tab_id))
                .unwrap_or(true)
            {
                workspace.set_primary(workspace.tab_order.first().copied());
            }

            workspace.active_tab_id = workspace.primary_tab_id;
            if workspace
                .secondary_tab_id
                .is_some_and(|tab_id| !workspace.tab_order.contains(&tab_id))
            {
                workspace.secondary_tab_id = None;
                workspace.split_enabled = false;
            }

            if let Some(active_tab_id) = workspace.primary_tab_id {
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

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::BTreeSet;
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
        live_tabs: Option<Rc<RefCell<BTreeSet<TabId>>>>,
    }

    impl RecordingHost {
        fn new(events: Rc<RefCell<Vec<HostEvent>>>) -> Self {
            Self {
                next_window_id: 0,
                next_ui_view_id: 0,
                next_content_view_id: 0,
                events,
                live_tabs: None,
            }
        }

        fn with_live_tracking(
            events: Rc<RefCell<Vec<HostEvent>>>,
            live_tabs: Rc<RefCell<BTreeSet<TabId>>>,
        ) -> Self {
            let mut host = Self::new(events);
            host.live_tabs = Some(live_tabs);
            host
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
            profile_id: switchboard_core::ProfileId,
            tab_id: TabId,
            generation: u64,
            url: &str,
        ) -> Result<ContentViewId, Self::Error> {
            self.next_content_view_id += 1;
            let view_id = ContentViewId(self.next_content_view_id);
            if let Some(live_tabs) = &self.live_tabs {
                live_tabs.borrow_mut().insert(tab_id);
            }
            self.events
                .borrow_mut()
                .push(HostEvent::ContentViewCreated {
                    window_id,
                    view_id,
                    profile_id,
                    tab_id,
                    generation,
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

        fn layout_content_views(
            &mut self,
            primary: Option<ContentViewId>,
            secondary: Option<ContentViewId>,
            ratio: f64,
            split_enabled: bool,
        ) -> Result<(), Self::Error> {
            self.events
                .borrow_mut()
                .push(HostEvent::ContentLayoutChanged {
                    primary,
                    secondary,
                    ratio_millis: (ratio.clamp(0.25, 0.75) * 1_000.0).round() as u16,
                    split_enabled,
                });
            Ok(())
        }

        fn set_focus_mode(&mut self, active: bool) -> Result<(), Self::Error> {
            self.events
                .borrow_mut()
                .push(HostEvent::FocusModeChanged { active });
            Ok(())
        }

        fn go_back(&mut self, view_id: ContentViewId) -> Result<(), Self::Error> {
            self.events
                .borrow_mut()
                .push(HostEvent::ContentHistoryAction {
                    view_id,
                    action: "back",
                });
            Ok(())
        }

        fn go_forward(&mut self, view_id: ContentViewId) -> Result<(), Self::Error> {
            self.events
                .borrow_mut()
                .push(HostEvent::ContentHistoryAction {
                    view_id,
                    action: "forward",
                });
            Ok(())
        }

        fn reload(&mut self, view_id: ContentViewId) -> Result<(), Self::Error> {
            self.events
                .borrow_mut()
                .push(HostEvent::ContentHistoryAction {
                    view_id,
                    action: "reload",
                });
            Ok(())
        }

        fn send_ui_message(&mut self, payload: &str) -> Result<(), Self::Error> {
            self.events.borrow_mut().push(HostEvent::UiMessageSent {
                payload: payload.to_owned(),
            });
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

        fn has_live_content_browser(&self, tab_id: TabId) -> bool {
            self.live_tabs
                .as_ref()
                .is_some_and(|live_tabs| live_tabs.borrow().contains(&tab_id))
        }

        fn run_event_loop(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    fn commit_pending<H: CefHost + 'static>(runtime: &mut AppRuntime<H>)
    where
        H::Error: std::fmt::Debug,
    {
        while let Some((tab_id, generation)) = runtime
            .pending_restores
            .iter()
            .next()
            .map(|(tab_id, pending)| (*tab_id, pending.generation))
        {
            runtime
                .handle_ui_command(UiCommand::FrameCommitted {
                    tab_id: tab_id.0,
                    generation,
                })
                .expect("pending frame should commit");
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
        commit_pending(&mut runtime);

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
        commit_pending(&mut runtime);

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
        commit_pending(&mut runtime);

        runtime
            .handle_ui_command(UiCommand::NewTab {
                workspace_id: workspace_id.0,
                url: Some("https://two.example".to_owned()),
                make_active: true,
            })
            .expect("second tab should succeed");
        commit_pending(&mut runtime);

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
        commit_pending(&mut runtime);
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
        commit_pending(&mut runtime);
        let generation = runtime.tab_bindings[&tab_id].content.generation;

        runtime
            .handle_content_event(ContentEvent::TitleChanged {
                tab_id,
                generation,
                title: "One Example".to_owned(),
            })
            .expect("title event should apply");
        runtime
            .handle_content_event(ContentEvent::LoadingChanged {
                tab_id,
                generation,
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
    fn history_is_recorded_only_after_successful_main_frame_load() {
        let host = MockCefHost::default();
        let mut runtime = AppRuntime::bootstrap(host, "0.1.0").expect("bootstrap should succeed");
        let workspace_id = runtime.default_workspace_id();

        runtime
            .handle_ui_command(UiCommand::NewTab {
                workspace_id: workspace_id.0,
                url: Some("https://history.example/path".to_owned()),
                make_active: true,
            })
            .expect("tab should be created");
        let tab_id = runtime
            .active_tab_id(workspace_id)
            .expect("tab should be active");
        commit_pending(&mut runtime);
        let generation = runtime.tab_bindings[&tab_id].content.generation;

        runtime
            .handle_content_event(ContentEvent::LoadingChanged {
                tab_id,
                generation,
                is_loading: false,
            })
            .expect("loading state should apply");
        assert!(runtime.engine().state().history.is_empty());

        runtime
            .handle_content_event(ContentEvent::MainFrameLoadSucceeded { tab_id, generation })
            .expect("successful load should record history");

        let profile_id = runtime.engine().state().tabs[&tab_id].profile_id;
        let history = &runtime.engine().state().history[&profile_id];
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].url, "https://history.example/path");
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
        commit_pending(&mut runtime);
        let generation = runtime.tab_bindings[&tab_id].content.generation;

        runtime
            .handle_ui_command(UiCommand::CloseTab { tab_id: tab_id.0 })
            .expect("close tab should succeed");
        let revision_before = runtime.revision();

        let patch = runtime
            .handle_content_event(ContentEvent::TitleChanged {
                tab_id,
                generation,
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

        let before_commit_content_creates = events
            .borrow()
            .iter()
            .filter(|event| matches!(event, HostEvent::ContentViewCreated { .. }))
            .count();
        assert_eq!(before_commit_content_creates, 0);
        assert_eq!(runtime.pending_restores.len(), 1);
        commit_pending(&mut runtime);
        let after_commit_content_creates = events
            .borrow()
            .iter()
            .filter(|event| matches!(event, HostEvent::ContentViewCreated { .. }))
            .count();
        assert_eq!(after_commit_content_creates, 1);
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
        commit_pending(&mut runtime);
        assert_eq!(runtime.revision(), expected_revision + 1);
        expected_revision = runtime.revision();

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
        assert!(!initial.contains("password_manager."));

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
            commit_pending(&mut runtime);

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
        commit_pending(&mut runtime);

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
        commit_pending(&mut runtime);

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
        let created_profiles: BTreeSet<_> = runtime
            .host()
            .events()
            .iter()
            .filter_map(|event| match event {
                HostEvent::ContentViewCreated { profile_id, .. } => Some(*profile_id),
                _ => None,
            })
            .collect();
        assert_eq!(created_profiles.len(), 2);
    }

    #[test]
    fn restore_waits_for_explicit_browser_close_completion() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let live_tabs = Rc::new(RefCell::new(BTreeSet::new()));
        let host = RecordingHost::with_live_tracking(events.clone(), live_tabs.clone());
        let mut runtime = AppRuntime::bootstrap(host, "0.1.0").expect("bootstrap should succeed");
        let default_profile_id = runtime.engine().state().active_profile_id.unwrap();
        let default_workspace_id = runtime.default_workspace_id();

        runtime
            .handle_ui_command(UiCommand::NewTab {
                workspace_id: default_workspace_id.0,
                url: Some("https://one.example".to_owned()),
                make_active: true,
            })
            .unwrap();
        let first_tab = runtime.active_tab_id(default_workspace_id).unwrap();
        commit_pending(&mut runtime);
        let first_generation = runtime.tab_bindings[&first_tab].content.generation;

        runtime
            .handle_ui_command(UiCommand::NewProfile {
                name: "Work".to_owned(),
            })
            .unwrap();
        let second_profile_id = runtime
            .engine()
            .state()
            .active_profile_id
            .filter(|profile_id| *profile_id != default_profile_id)
            .unwrap();
        let second_workspace_id = runtime.engine().state().profiles[&second_profile_id]
            .active_workspace_id
            .unwrap();
        runtime
            .handle_ui_command(UiCommand::NewTab {
                workspace_id: second_workspace_id.0,
                url: Some("https://two.example".to_owned()),
                make_active: true,
            })
            .unwrap();
        commit_pending(&mut runtime);

        runtime
            .handle_ui_command(UiCommand::SwitchProfile {
                profile_id: default_profile_id.0,
            })
            .unwrap();
        let restore_generation = runtime.pending_restores[&first_tab].generation;
        runtime
            .handle_ui_command(UiCommand::FrameCommitted {
                tab_id: first_tab.0,
                generation: restore_generation,
            })
            .unwrap();
        assert!(runtime.pending_restores.contains_key(&first_tab));
        assert_eq!(
            events
                .borrow()
                .iter()
                .filter(|event| matches!(event, HostEvent::ContentViewCreated { .. }))
                .count(),
            2
        );

        live_tabs.borrow_mut().remove(&first_tab);
        runtime
            .handle_content_event(ContentEvent::BrowserClosed {
                tab_id: first_tab,
                generation: first_generation,
            })
            .unwrap();
        runtime
            .handle_ui_command(UiCommand::FrameCommitted {
                tab_id: first_tab.0,
                generation: restore_generation,
            })
            .unwrap();

        assert!(!runtime.pending_restores.contains_key(&first_tab));
        assert_eq!(
            events
                .borrow()
                .iter()
                .filter(|event| matches!(event, HostEvent::ContentViewCreated { .. }))
                .count(),
            3
        );
    }

    #[test]
    fn loading_complete_does_not_persist_synthetic_thumbnail() {
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
        commit_pending(&mut runtime);
        let generation = runtime.tab_bindings[&tab_id].content.generation;

        runtime
            .handle_content_event(ContentEvent::LoadingChanged {
                tab_id,
                generation,
                is_loading: false,
            })
            .expect("loading complete should update metadata");

        let state_json = runtime.ui_shell_state_json();
        assert!(!state_json.contains("thumbnail_data_url"));
    }
}
