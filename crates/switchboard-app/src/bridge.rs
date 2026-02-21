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
    NavigateActive {
        url: String,
    },
    NewWorkspace {
        name: String,
    },
    RenameWorkspace {
        workspace_id: u64,
        name: String,
    },
    DeleteWorkspace {
        workspace_id: u64,
    },
    SwitchWorkspace {
        workspace_id: u64,
    },
    ActivateTab {
        tab_id: u64,
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
            Self::NavigateActive { .. } => {
                unreachable!(
                    "NavigateActive requires runtime tab resolution before intent dispatch"
                )
            }
            Self::NewWorkspace { .. } => {
                unreachable!(
                    "NewWorkspace requires runtime profile resolution before intent dispatch"
                )
            }
            Self::RenameWorkspace { workspace_id, name } => Intent::RenameWorkspace {
                workspace_id: WorkspaceId(workspace_id),
                name,
            },
            Self::DeleteWorkspace { workspace_id } => Intent::DeleteWorkspace {
                workspace_id: WorkspaceId(workspace_id),
            },
            Self::SwitchWorkspace { workspace_id } => Intent::SwitchWorkspace {
                workspace_id: WorkspaceId(workspace_id),
            },
            Self::ActivateTab { tab_id } => Intent::ActivateTab {
                tab_id: TabId(tab_id),
            },
        }
    }
}
