use std::convert::Infallible;
use std::error::Error;
use std::fmt::{Display, Formatter, Result as FmtResult};

use switchboard_core::{
    BrowserState, Engine, EngineError, Intent, NoopPersistence, Patch, TabId, WorkspaceId,
};

use crate::bridge::UiCommand;
use crate::host::{install_ui_command_handler, CefHost, ContentViewId, UiViewId, WindowId};

const UI_SHELL_URL: &str = "app://ui";

#[derive(Debug)]
pub enum RuntimeError<HError> {
    Host(HError),
    Engine(EngineError<Infallible>),
    NoActiveTab,
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
        let ui_view_id = host
            .create_ui_view(window_id, UI_SHELL_URL)
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

        let result = self.host.run_event_loop().map_err(RuntimeError::Host);
        install_ui_command_handler(None);
        result
    }

    pub fn handle_ui_command(
        &mut self,
        command: UiCommand,
    ) -> Result<Patch, RuntimeError<H::Error>> {
        match command {
            UiCommand::NavigateActive { url } => {
                let tab_id = self
                    .resolve_active_tab_id()
                    .ok_or(RuntimeError::NoActiveTab)?;
                self.handle_intent(Intent::Navigate { tab_id, url })
            }
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
            Self::NoActiveTab => write!(f, "no active tab available for UI navigation"),
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
            HostEvent::UiViewCreated { url, .. } if url == "app://ui"
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
        assert_eq!(content_navigate_count, 1);
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
}
