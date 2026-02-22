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
    ObserveTabUrl {
        tab_id: TabId,
        url: String,
    },
    ObserveTabTitle {
        tab_id: TabId,
        title: String,
    },
    ObserveTabLoading {
        tab_id: TabId,
        is_loading: bool,
    },
    ObserveTabThumbnail {
        tab_id: TabId,
        data_url: Option<String>,
    },
    NewProfile {
        name: String,
    },
    DeleteProfile {
        profile_id: ProfileId,
    },
    RenameProfile {
        profile_id: ProfileId,
        name: String,
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
    DeleteWorkspace {
        workspace_id: WorkspaceId,
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
