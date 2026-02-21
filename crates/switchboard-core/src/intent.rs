use crate::ids::{ProfileId, TabId, WorkspaceId};
use crate::state::SettingValue;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Intent {
    UiReady {
        ui_version: String,
    },
    FrameCommitted {
        revision: u64,
    },
    Navigate {
        tab_id: TabId,
        url: String,
    },
    NewTab {
        workspace_id: WorkspaceId,
        url: Option<String>,
        make_active: bool,
    },
    CloseTab {
        tab_id: TabId,
    },
    ActivateTab {
        tab_id: TabId,
    },
    MoveTab {
        tab_id: TabId,
        workspace_id: WorkspaceId,
        index: usize,
    },
    NewWorkspace {
        profile_id: ProfileId,
        name: String,
    },
    RenameWorkspace {
        workspace_id: WorkspaceId,
        name: String,
    },
    SwitchWorkspace {
        workspace_id: WorkspaceId,
    },
    SwitchProfile {
        profile_id: ProfileId,
    },
    PinTab {
        tab_id: TabId,
        pinned: bool,
    },
    DiscardTab {
        tab_id: TabId,
    },
    SettingSet {
        key: String,
        value: SettingValue,
    },
}
