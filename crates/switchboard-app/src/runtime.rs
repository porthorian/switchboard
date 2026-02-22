use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter, Result as FmtResult};

use switchboard_core::{
    BrowserState, Engine, EngineError, Intent, Patch, PatchOp, ProfileId, TabRuntimeState, TabId,
    WorkspaceId,
};
#[cfg(test)]
use std::convert::Infallible;
#[cfg(test)]
use switchboard_core::NoopPersistence;

use crate::bridge::UiCommand;
use crate::host::{
    install_content_event_handler, install_ui_command_handler, install_ui_state_provider, CefHost,
    ContentEvent, ContentViewId, UiViewId, WindowId,
};
#[cfg(not(test))]
use crate::persistence::{AppPersistence, AppPersistenceError};

const UI_SHELL_URL_BASE: &str = "app://ui";
const THUMBNAIL_MAX_ENTRIES: usize = 120;

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
    WorkspaceNotFound(WorkspaceId),
    TabNotFound(TabId),
    BlockedContentNavigation(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ContentBinding {
    view_id: ContentViewId,
    tab_id: TabId,
}

pub struct AppRuntime<H: CefHost> {
    engine: Engine<RuntimePersistence>,
    host: H,
    window_id: WindowId,
    ui_view_id: UiViewId,
    default_workspace_id: WorkspaceId,
    content_bindings: BTreeMap<ProfileId, ContentBinding>,
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
        let mut engine = Engine::with_state(persistence, state, 0);

        let window_id = host
            .create_window("Switchboard")
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
            content_bindings: BTreeMap::new(),
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

        let result = self.host.run_event_loop().map_err(RuntimeError::Host);
        install_ui_command_handler(None);
        install_ui_state_provider(None);
        install_content_event_handler(None);
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
            other => self.handle_intent(other.into_intent()),
        }
    }

    pub fn handle_intent(&mut self, intent: Intent) -> Result<Patch, RuntimeError<H::Error>> {
        let navigation = match &intent {
            Intent::Navigate { tab_id, url } => {
                if url.starts_with("app://") {
                    return Err(RuntimeError::BlockedContentNavigation(url.clone()));
                }
                Some((*tab_id, url.clone()))
            }
            _ => None,
        };

        let patch = self.engine.dispatch(intent).map_err(RuntimeError::Engine)?;

        if let Some((tab_id, url)) = navigation {
            self.ensure_single_content_view(tab_id, &url)?;
        } else if patch_updates_active_content(&patch) {
            self.sync_active_profile_content_view()?;
        }

        self.cleanup_deleted_profile_bindings()?;
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

    fn resolve_active_tab_target(&self) -> Option<(TabId, String)> {
        let tab_id = self.resolve_active_tab_id()?;
        let url = self.engine.state().tabs.get(&tab_id)?.url.clone();
        Some((tab_id, url))
    }

    fn resolve_active_profile_id(&self) -> Option<ProfileId> {
        self.engine.state().active_profile_id
    }

    fn sync_active_profile_content_view(&mut self) -> Result<(), RuntimeError<H::Error>> {
        let active_profile_id = match self.resolve_active_profile_id() {
            Some(profile_id) => profile_id,
            None => {
                self.set_profile_view_visibility(None)?;
                return Ok(());
            }
        };

        if let Some((tab_id, url)) = self.resolve_active_tab_target() {
            self.ensure_single_content_view(tab_id, &url)?;
            self.set_profile_view_visibility(Some(active_profile_id))?;
            return Ok(());
        }

        if let Some(binding) = self.content_bindings.get(&active_profile_id).copied() {
            self.host
                .clear_content_view(binding.view_id)
                .map_err(RuntimeError::Host)?;
            self.host
                .set_content_view_visible(binding.view_id, false)
                .map_err(RuntimeError::Host)?;
        }

        self.set_profile_view_visibility(Some(active_profile_id))?;
        Ok(())
    }

    fn set_profile_view_visibility(
        &mut self,
        active_profile_id: Option<ProfileId>,
    ) -> Result<(), RuntimeError<H::Error>> {
        let views: Vec<(ProfileId, ContentViewId)> = self
            .content_bindings
            .iter()
            .map(|(profile_id, binding)| (*profile_id, binding.view_id))
            .collect();
        for (profile_id, view_id) in views {
            let visible = active_profile_id == Some(profile_id);
            self.host
                .set_content_view_visible(view_id, visible)
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

    fn cleanup_deleted_profile_bindings(&mut self) -> Result<(), RuntimeError<H::Error>> {
        let existing_profile_ids: std::collections::BTreeSet<ProfileId> =
            self.engine.state().profiles.keys().copied().collect();
        let removed_profile_ids: Vec<ProfileId> = self
            .content_bindings
            .keys()
            .copied()
            .filter(|profile_id| !existing_profile_ids.contains(profile_id))
            .collect();

        for profile_id in removed_profile_ids {
            if let Some(binding) = self.content_bindings.remove(&profile_id) {
                self.host
                    .clear_content_view(binding.view_id)
                    .map_err(RuntimeError::Host)?;
                self.host
                    .set_content_view_visible(binding.view_id, false)
                    .map_err(RuntimeError::Host)?;
            }
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
        json.push_str("]}");
        json
    }

    fn ensure_single_content_view(
        &mut self,
        tab_id: TabId,
        url: &str,
    ) -> Result<(), RuntimeError<H::Error>> {
        let tab = self
            .engine
            .state()
            .tabs
            .get(&tab_id)
            .ok_or(RuntimeError::TabNotFound(tab_id))?;
        let workspace_id = tab.workspace_id;
        let profile_id = tab.profile_id;
        if !self.engine.state().workspaces.contains_key(&workspace_id) {
            return Err(RuntimeError::WorkspaceNotFound(workspace_id));
        }

        match self.content_bindings.get(&profile_id).copied() {
            Some(mut binding) => {
                self.host
                    .navigate_content_view(binding.view_id, tab_id, url)
                    .map_err(RuntimeError::Host)?;
                self.host
                    .set_content_view_visible(
                        binding.view_id,
                        self.resolve_active_profile_id() == Some(profile_id),
                    )
                    .map_err(RuntimeError::Host)?;
                binding.tab_id = tab_id;
                self.content_bindings.insert(profile_id, binding);
            }
            None => {
                let view_id = self
                    .host
                    .create_content_view(self.window_id, tab_id, url)
                    .map_err(RuntimeError::Host)?;
                self.host
                    .set_content_view_visible(
                        view_id,
                        self.resolve_active_profile_id() == Some(profile_id),
                    )
                    .map_err(RuntimeError::Host)?;
                self.content_bindings
                    .insert(profile_id, ContentBinding { view_id, tab_id });
            }
        }

        Ok(())
    }
}

fn ensure_bootstrap_state(state: &mut BrowserState) -> WorkspaceId {
    if state.profiles.is_empty() {
        let profile_id = state.add_profile("Default");
        let workspace_id = state
            .add_workspace(profile_id, "Workspace 1")
            .expect("bootstrap profile must exist");
        state.recompute_next_ids();
        return workspace_id;
    }

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

    let profile_id = state.active_profile_id.unwrap_or_else(|| state.add_profile("Default"));
    let workspace_id = state
        .add_workspace(profile_id, "Workspace 1")
        .expect("profile must exist");
    state.active_profile_id = Some(profile_id);
    state.recompute_next_ids();
    workspace_id
}

impl<HError: Display> Display for RuntimeError<HError> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::PersistenceInit(message) => write!(f, "persistence initialization failed: {message}"),
            Self::Host(err) => write!(f, "host error: {err}"),
            Self::Engine(err) => write!(f, "engine error: {err:?}"),
            Self::NoActiveWorkspace => {
                write!(f, "no active workspace available for UI navigation")
            }
            Self::NoActiveProfile => write!(f, "no active profile available for UI command"),
            Self::WorkspaceNotFound(workspace_id) => {
                write!(f, "workspace not found: {workspace_id}")
            }
            Self::TabNotFound(tab_id) => write!(f, "tab not found: {tab_id}"),
            Self::BlockedContentNavigation(url) => {
                write!(f, "content navigation blocked for url: {url}")
            }
        }
    }
}

impl<HError: Error + 'static> Error for RuntimeError<HError> {}

fn patch_updates_active_content(patch: &Patch) -> bool {
    patch.ops.iter().any(|op| {
        matches!(
            op,
            PatchOp::SetActiveProfile { .. }
                | PatchOp::SetActiveWorkspace { .. }
                | PatchOp::SetActiveTab { .. }
        )
    })
}

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
    format!("data:image/svg+xml;utf8,{}", percent_encode_uri_component(&svg))
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

#[cfg(test)]
mod tests {
    use crate::bridge::UiCommand;
    use crate::host::{ContentEvent, HostEvent, MockCefHost};

    use super::{AppRuntime, RuntimeError};

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

        let content_navigate_count = runtime
            .host()
            .events()
            .iter()
            .filter(|event| matches!(event, HostEvent::ContentNavigated { .. }))
            .count();
        assert_eq!(content_navigate_count, 2);
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
        assert_eq!(content_create_count, 2);
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
