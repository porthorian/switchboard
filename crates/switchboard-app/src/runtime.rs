use std::convert::Infallible;
use std::error::Error;
use std::fmt::{Display, Formatter, Result as FmtResult};

use switchboard_core::{
    BrowserState, Engine, EngineError, Intent, NoopPersistence, Patch, ProfileId, TabId,
    WorkspaceId,
};

use crate::bridge::UiCommand;
use crate::host::{
    install_ui_command_handler, install_ui_state_provider, CefHost, ContentViewId, UiViewId,
    WindowId,
};

const UI_SHELL_URL_BASE: &str = "app://ui";

#[derive(Debug)]
pub enum RuntimeError<HError> {
    Host(HError),
    Engine(EngineError<Infallible>),
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
    engine: Engine<NoopPersistence>,
    host: H,
    window_id: WindowId,
    ui_view_id: UiViewId,
    default_workspace_id: WorkspaceId,
    content_binding: Option<ContentBinding>,
}

impl<H: CefHost + 'static> AppRuntime<H> {
    pub fn bootstrap(mut host: H, ui_version: &str) -> Result<Self, RuntimeError<H::Error>> {
        let mut state = BrowserState::default();
        let profile_id = state.add_profile("Default");
        let workspace_id = state
            .add_workspace(profile_id, "Workspace 1")
            .expect("bootstrap profile must exist");

        let mut engine = Engine::with_state(NoopPersistence, state, 0);

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
            content_binding: None,
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

    pub fn engine(&self) -> &Engine<NoopPersistence> {
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
        install_ui_state_provider(Some(Box::new(move || unsafe {
            (*runtime_ptr).ui_shell_state_json()
        })));

        let result = self.host.run_event_loop().map_err(RuntimeError::Host);
        install_ui_command_handler(None);
        install_ui_state_provider(None);
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
            other => self.handle_intent(other.into_intent()),
        }
    }

    pub fn handle_intent(&mut self, intent: Intent) -> Result<Patch, RuntimeError<H::Error>> {
        let should_sync_active_content = matches!(
            &intent,
            Intent::NewTab {
                make_active: true,
                ..
            } | Intent::ActivateTab { .. }
                | Intent::SwitchWorkspace { .. }
                | Intent::SwitchProfile { .. }
        );
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
        } else if should_sync_active_content {
            if let Some((tab_id, url)) = self.resolve_active_tab_target() {
                self.ensure_single_content_view(tab_id, &url)?;
            }
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
            json.push_str("\"workspace_id\":");
            json.push_str(&tab.workspace_id.0.to_string());
            json.push(',');
            json.push_str("\"url\":");
            push_json_string(&mut json, &tab.url);
            json.push(',');
            json.push_str("\"title\":");
            push_json_string(&mut json, &tab.title);
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
        if !self.engine.state().workspaces.contains_key(&workspace_id) {
            return Err(RuntimeError::WorkspaceNotFound(workspace_id));
        }

        match self.content_binding {
            Some(mut binding) => {
                self.host
                    .navigate_content_view(binding.view_id, tab_id, url)
                    .map_err(RuntimeError::Host)?;
                binding.tab_id = tab_id;
                self.content_binding = Some(binding);
            }
            None => {
                let view_id = self
                    .host
                    .create_content_view(self.window_id, tab_id, url)
                    .map_err(RuntimeError::Host)?;
                self.content_binding = Some(ContentBinding { view_id, tab_id });
            }
        }

        Ok(())
    }
}

impl<HError: Display> Display for RuntimeError<HError> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
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
    use crate::host::{HostEvent, MockCefHost};

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
}
