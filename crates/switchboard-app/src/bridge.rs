use switchboard_core::{Intent, TabId, WorkspaceId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiCommand {
    UiReady {
        ui_version: String,
    },
    NewTab {
        workspace_id: u64,
        url: Option<String>,
        make_active: bool,
    },
    Navigate {
        tab_id: u64,
        url: String,
    },
}

impl UiCommand {
    pub fn into_intent(self) -> Intent {
        match self {
            Self::UiReady { ui_version } => Intent::UiReady { ui_version },
            Self::NewTab {
                workspace_id,
                url,
                make_active,
            } => Intent::NewTab {
                workspace_id: WorkspaceId(workspace_id),
                url,
                make_active,
            },
            Self::Navigate { tab_id, url } => Intent::Navigate {
                tab_id: TabId(tab_id),
                url,
            },
        }
    }
}
