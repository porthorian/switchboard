use serde::{Deserialize, Serialize};

use crate::ids::{ProfileId, TabId, WorkspaceId};
use crate::state::SettingValue;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClearHistoryScope {
    Profile(ProfileId),
    All,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Intent {
    UiReady {
        ui_version: String,
    },
    FrameCommitted {
        revision: u64,
    },
    TabFrameCommitted {
        tab_id: TabId,
        generation: u64,
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
    ObserveTabNavigationState {
        tab_id: TabId,
        can_go_back: bool,
        can_go_forward: bool,
    },
    /// Retained as a no-op compatibility message; synthetic thumbnails are not persisted.
    ObserveTabThumbnail {
        tab_id: TabId,
        data_url: Option<String>,
    },
    RecordHistory {
        profile_id: ProfileId,
        url: String,
        title: String,
        visited_at_ms: i64,
    },
    ClearHistory {
        scope: ClearHistoryScope,
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
    NewSubtab {
        workspace_id: WorkspaceId,
        parent_tab_id: TabId,
        url: String,
        make_active: bool,
    },
    OpenLinkFromTab {
        source_tab_id: TabId,
        url: String,
        secondary: bool,
    },
    /// Compatibility alias for `CompleteTab`.
    CloseTab {
        tab_id: TabId,
    },
    CompleteTab {
        tab_id: TabId,
        completed_at_ms: i64,
    },
    RestoreTab {
        tab_id: TabId,
    },
    SnoozeTab {
        tab_id: TabId,
        wake_at_ms: i64,
    },
    WakeSnoozed {
        now_ms: i64,
    },
    PermanentlyDeleteTab {
        tab_id: TabId,
    },
    ClearArchive {
        profile_id: ProfileId,
    },
    RenameTab {
        tab_id: TabId,
        title: String,
    },
    SetTabLocked {
        tab_id: TabId,
        locked: bool,
    },
    ActivateTab {
        tab_id: TabId,
    },
    MoveTab {
        tab_id: TabId,
        workspace_id: WorkspaceId,
        index: usize,
    },
    MoveTabTree {
        tab_id: TabId,
        workspace_id: WorkspaceId,
        parent_tab_id: Option<TabId>,
        sibling_index: usize,
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
    SetSecondaryTab {
        workspace_id: WorkspaceId,
        tab_id: TabId,
    },
    ClearSplit {
        workspace_id: WorkspaceId,
    },
    SwapSplit {
        workspace_id: WorkspaceId,
    },
    SetSplitRatio {
        workspace_id: WorkspaceId,
        ratio: f64,
    },
    SettingSet {
        key: String,
        value: SettingValue,
    },
}
