use serde::{Deserialize, Serialize};

use crate::ids::{ProfileId, TabId, WorkspaceId};
use crate::state::{BrowserState, Profile, SettingValue, Tab, Workspace};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub state: BrowserState,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Patch {
    pub ops: Vec<PatchOp>,
    pub from_revision: u64,
    pub to_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PatchOp {
    UpsertProfile(Profile),
    UpsertWorkspace(Workspace),
    UpsertTab(Tab),
    RemoveTab {
        tab_id: TabId,
        workspace_id: WorkspaceId,
    },
    RemoveWorkspace {
        workspace_id: WorkspaceId,
        profile_id: ProfileId,
    },
    RemoveProfile {
        profile_id: ProfileId,
    },
    SetActiveProfile {
        profile_id: ProfileId,
    },
    SetActiveWorkspace {
        profile_id: ProfileId,
        workspace_id: WorkspaceId,
    },
    SetActiveTab {
        workspace_id: WorkspaceId,
        tab_id: Option<TabId>,
    },
    SettingChanged {
        key: String,
        value: SettingValue,
    },
    HistoryChanged {
        profile_id: Option<ProfileId>,
    },
}
