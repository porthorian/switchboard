use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::ids::{ProfileId, TabId, WorkspaceId};

/// Runtime residency is derived after startup and is never persisted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TabRuntimeState {
    Active,
    Secondary,
    Warm,
    Discarded,
    Restoring,
}

impl Default for TabRuntimeState {
    fn default() -> Self {
        Self::Discarded
    }
}

impl TabRuntimeState {
    pub fn is_visible(self) -> bool {
        matches!(self, Self::Active | Self::Secondary)
    }

    pub fn is_live(self) -> bool {
        matches!(self, Self::Active | Self::Secondary | Self::Warm)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestorePosition {
    pub parent_tab_id: Option<TabId>,
    pub sibling_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TabStatus {
    Open,
    Done {
        completed_at_ms: i64,
        restore: RestorePosition,
    },
    Snoozed {
        wake_at_ms: i64,
        restore: RestorePosition,
    },
}

impl Default for TabStatus {
    fn default() -> Self {
        Self::Open
    }
}

impl TabStatus {
    pub fn is_open(&self) -> bool {
        matches!(self, Self::Open)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub id: ProfileId,
    pub name: String,
    pub workspace_order: Vec<WorkspaceId>,
    pub active_workspace_id: Option<WorkspaceId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Workspace {
    pub id: WorkspaceId,
    pub profile_id: ProfileId,
    pub name: String,
    /// Depth-first order of open tabs. Tree ownership lives on `Tab::parent_tab_id`.
    pub tab_order: Vec<TabId>,
    /// Compatibility alias for the primary pane while the native host migrates.
    pub active_tab_id: Option<TabId>,
    pub primary_tab_id: Option<TabId>,
    pub secondary_tab_id: Option<TabId>,
    pub split_enabled: bool,
    pub split_ratio: f64,
}

impl Workspace {
    pub fn set_primary(&mut self, tab_id: Option<TabId>) {
        self.primary_tab_id = tab_id;
        self.active_tab_id = tab_id;
    }

    pub fn visible_tab_ids(&self) -> impl Iterator<Item = TabId> {
        let secondary = self
            .split_enabled
            .then_some(self.secondary_tab_id)
            .flatten();
        self.primary_tab_id.into_iter().chain(secondary)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tab {
    pub id: TabId,
    pub profile_id: ProfileId,
    pub workspace_id: WorkspaceId,
    pub parent_tab_id: Option<TabId>,
    pub url: String,
    /// Current display title retained for compatibility with the existing UI.
    pub title: String,
    pub observed_title: String,
    pub custom_title: Option<String>,
    pub pinned: bool,
    pub locked: bool,
    pub muted: bool,
    pub status: TabStatus,
    #[serde(default)]
    pub loading: bool,
    #[serde(default)]
    pub can_go_back: bool,
    #[serde(default)]
    pub can_go_forward: bool,
    #[serde(default)]
    pub runtime_state: TabRuntimeState,
}

impl Tab {
    pub fn refresh_display_title(&mut self) {
        self.title = self
            .custom_title
            .as_deref()
            .unwrap_or(&self.observed_title)
            .to_owned();
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub profile_id: ProfileId,
    pub url: String,
    pub title: String,
    pub visit_count: u64,
    pub last_visit_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SettingValue {
    Bool(bool),
    Int(i64),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateError {
    ProfileNotFound(ProfileId),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrowserState {
    pub profiles: BTreeMap<ProfileId, Profile>,
    pub workspaces: BTreeMap<WorkspaceId, Workspace>,
    pub tabs: BTreeMap<TabId, Tab>,
    pub settings: BTreeMap<String, SettingValue>,
    /// History is queried through Rust and intentionally omitted from UI snapshots.
    #[serde(default, skip_serializing)]
    pub history: BTreeMap<ProfileId, Vec<HistoryEntry>>,
    /// Runtime-only warm-pool LRU per profile (oldest -> newest).
    #[serde(default, skip_serializing)]
    pub warm_lru: BTreeMap<ProfileId, Vec<TabId>>,
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
            history: BTreeMap::new(),
            warm_lru: BTreeMap::new(),
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
                primary_tab_id: None,
                secondary_tab_id: None,
                split_enabled: false,
                split_ratio: 0.5,
            },
        );
        let profile = self.profiles.get_mut(&profile_id).expect("profile checked");
        profile.workspace_order.push(workspace_id);
        profile.active_workspace_id.get_or_insert(workspace_id);
        Ok(workspace_id)
    }

    pub fn active_workspace_id(&self) -> Option<WorkspaceId> {
        let profile_id = self.active_profile_id?;
        self.profiles.get(&profile_id)?.active_workspace_id
    }

    pub fn visible_tab_ids(&self) -> BTreeSet<TabId> {
        let Some(workspace_id) = self.active_workspace_id() else {
            return BTreeSet::new();
        };
        self.workspaces
            .get(&workspace_id)
            .map(|workspace| workspace.visible_tab_ids().collect())
            .unwrap_or_default()
    }

    pub(crate) fn allocate_workspace_id(&mut self) -> WorkspaceId {
        let id = WorkspaceId(self.next_workspace_id);
        self.next_workspace_id = self.next_workspace_id.saturating_add(1);
        id
    }

    pub(crate) fn allocate_tab_id(&mut self) -> TabId {
        let id = TabId(self.next_tab_id);
        self.next_tab_id = self.next_tab_id.saturating_add(1);
        id
    }

    fn allocate_profile_id(&mut self) -> ProfileId {
        let id = ProfileId(self.next_profile_id);
        self.next_profile_id = self.next_profile_id.saturating_add(1);
        id
    }

    pub fn touch_warm_lru(&mut self, profile_id: ProfileId, tab_id: TabId) {
        let entries = self.warm_lru.entry(profile_id).or_default();
        entries.retain(|candidate| *candidate != tab_id);
        entries.push(tab_id);
    }

    pub fn remove_from_warm_lru(&mut self, profile_id: ProfileId, tab_id: TabId) {
        if let Some(entries) = self.warm_lru.get_mut(&profile_id) {
            entries.retain(|candidate| *candidate != tab_id);
            if entries.is_empty() {
                self.warm_lru.remove(&profile_id);
            }
        }
    }

    pub fn prune_warm_lru(&mut self) {
        self.warm_lru.retain(|profile_id, tab_ids| {
            tab_ids.retain(|tab_id| {
                self.tabs
                    .get(tab_id)
                    .map(|tab| tab.profile_id == *profile_id && tab.status.is_open())
                    .unwrap_or(false)
            });
            !tab_ids.is_empty() && self.profiles.contains_key(profile_id)
        });
    }

    pub fn recompute_next_ids(&mut self) {
        self.next_profile_id = self
            .profiles
            .keys()
            .next_back()
            .map(|id| id.0.saturating_add(1))
            .unwrap_or(1);
        self.next_workspace_id = self
            .workspaces
            .keys()
            .next_back()
            .map(|id| id.0.saturating_add(1))
            .unwrap_or(1);
        self.next_tab_id = self
            .tabs
            .keys()
            .next_back()
            .map(|id| id.0.saturating_add(1))
            .unwrap_or(1);
    }

    pub fn reset_runtime_state(&mut self) {
        self.warm_lru.clear();
        for tab in self.tabs.values_mut() {
            tab.loading = false;
            tab.can_go_back = false;
            tab.can_go_forward = false;
            tab.runtime_state = TabRuntimeState::Discarded;
        }
    }
}
