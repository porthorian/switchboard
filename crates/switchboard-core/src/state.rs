use std::collections::BTreeMap;

use crate::ids::{ProfileId, TabId, WorkspaceId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabRuntimeState {
    Active,
    Warm,
    Discarded,
    Restoring,
}

impl Default for TabRuntimeState {
    fn default() -> Self {
        Self::Discarded
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub id: ProfileId,
    pub name: String,
    pub workspace_order: Vec<WorkspaceId>,
    pub active_workspace_id: Option<WorkspaceId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    pub id: WorkspaceId,
    pub profile_id: ProfileId,
    pub name: String,
    pub tab_order: Vec<TabId>,
    pub active_tab_id: Option<TabId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tab {
    pub id: TabId,
    pub profile_id: ProfileId,
    pub workspace_id: WorkspaceId,
    pub url: String,
    pub title: String,
    pub loading: bool,
    pub thumbnail_data_url: Option<String>,
    pub pinned: bool,
    pub muted: bool,
    pub runtime_state: TabRuntimeState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingValue {
    Bool(bool),
    Int(i64),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateError {
    ProfileNotFound(ProfileId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserState {
    pub profiles: BTreeMap<ProfileId, Profile>,
    pub workspaces: BTreeMap<WorkspaceId, Workspace>,
    pub tabs: BTreeMap<TabId, Tab>,
    pub settings: BTreeMap<String, SettingValue>,
    pub active_profile_id: Option<ProfileId>,
    next_profile_id: u64,
    next_workspace_id: u64,
    next_tab_id: u64,
}

impl Default for BrowserState {
    fn default() -> Self {
        Self {
            profiles: BTreeMap::new(),
            workspaces: BTreeMap::new(),
            tabs: BTreeMap::new(),
            settings: BTreeMap::new(),
            active_profile_id: None,
            next_profile_id: 1,
            next_workspace_id: 1,
            next_tab_id: 1,
        }
    }
}

impl BrowserState {
    pub fn add_profile(&mut self, name: impl Into<String>) -> ProfileId {
        let profile_id = self.allocate_profile_id();
        self.profiles.insert(
            profile_id,
            Profile {
                id: profile_id,
                name: name.into(),
                workspace_order: Vec::new(),
                active_workspace_id: None,
            },
        );
        if self.active_profile_id.is_none() {
            self.active_profile_id = Some(profile_id);
        }
        profile_id
    }

    pub fn add_workspace(
        &mut self,
        profile_id: ProfileId,
        name: impl Into<String>,
    ) -> Result<WorkspaceId, StateError> {
        if !self.profiles.contains_key(&profile_id) {
            return Err(StateError::ProfileNotFound(profile_id));
        }

        let workspace_id = self.allocate_workspace_id();
        self.workspaces.insert(
            workspace_id,
            Workspace {
                id: workspace_id,
                profile_id,
                name: name.into(),
                tab_order: Vec::new(),
                active_tab_id: None,
            },
        );

        let profile = self.profiles.get_mut(&profile_id).expect("checked above");
        profile.workspace_order.push(workspace_id);
        if profile.active_workspace_id.is_none() {
            profile.active_workspace_id = Some(workspace_id);
        }

        Ok(workspace_id)
    }

    pub fn active_workspace_id(&self) -> Option<WorkspaceId> {
        let active_profile = self.active_profile_id?;
        self.profiles
            .get(&active_profile)
            .and_then(|profile| profile.active_workspace_id)
    }

    pub(crate) fn allocate_workspace_id(&mut self) -> WorkspaceId {
        let id = WorkspaceId(self.next_workspace_id);
        self.next_workspace_id += 1;
        id
    }

    pub(crate) fn allocate_tab_id(&mut self) -> TabId {
        let id = TabId(self.next_tab_id);
        self.next_tab_id += 1;
        id
    }

    fn allocate_profile_id(&mut self) -> ProfileId {
        let id = ProfileId(self.next_profile_id);
        self.next_profile_id += 1;
        id
    }
}
