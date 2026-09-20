use std::collections::{BTreeMap, BTreeSet};

use crate::ids::{ProfileId, TabId, WorkspaceId};
use crate::intent::{ClearHistoryScope, Intent};
use crate::patch::PatchOp;
use crate::state::{
    BrowserState, HistoryEntry, RestorePosition, SettingValue, Tab, TabRuntimeState, TabStatus,
};

const WARM_POOL_BUDGET_KEY: &str = "warm_pool_budget";
const DEFAULT_WARM_POOL_BUDGET: usize = 8;
const MAX_WARM_POOL_BUDGET: usize = 32;
const HOMEPAGE_KEY: &str = "homepage";
const NEW_TAB_BEHAVIOR_KEY: &str = "new_tab_behavior";
const NEW_TAB_CUSTOM_URL_KEY: &str = "new_tab_custom_url";
const ARCHIVE_MAX_AGE_MS: i64 = 30 * 24 * 60 * 60 * 1_000;
const ARCHIVE_MAX_ENTRIES_PER_PROFILE: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReduceError {
    ProfileNotFound(ProfileId),
    WorkspaceNotFound(WorkspaceId),
    TabNotFound(TabId),
    CannotDeleteLastProfile(ProfileId),
    CannotDeleteLastWorkspace(WorkspaceId),
    CrossProfileMove {
        tab_id: TabId,
        from_profile: ProfileId,
        to_profile: ProfileId,
    },
    InvalidParent {
        tab_id: TabId,
        parent_tab_id: TabId,
    },
    InvalidSiblingPosition {
        tab_id: TabId,
        sibling_index: usize,
    },
    PinnedOrderViolation {
        tab_id: TabId,
        sibling_index: usize,
    },
    TreeCycle {
        tab_id: TabId,
        parent_tab_id: TabId,
    },
    TabLocked(TabId),
    TabNotOpen(TabId),
    CannotDiscardVisibleTab(TabId),
    InvalidSplitTab {
        workspace_id: WorkspaceId,
        tab_id: TabId,
    },
    InvalidSplitRatio,
    AmbiguousKeybinding(String),
}

pub fn apply_intent(state: &mut BrowserState, intent: Intent) -> Result<Vec<PatchOp>, ReduceError> {
    let mut ops = Vec::new();
    let mut enforce = false;

    match intent {
        Intent::UiReady { .. } => enforce = true,
        Intent::FrameCommitted { .. } => {}
        Intent::TabFrameCommitted { tab_id, .. } => {
            let restored_state = state
                .tabs
                .get(&tab_id)
                .and_then(|tab| state.workspaces.get(&tab.workspace_id))
                .filter(|workspace| {
                    workspace.split_enabled && workspace.secondary_tab_id == Some(tab_id)
                })
                .map(|_| TabRuntimeState::Secondary)
                .unwrap_or(TabRuntimeState::Active);
            let tab = state
                .tabs
                .get_mut(&tab_id)
                .ok_or(ReduceError::TabNotFound(tab_id))?;
            if tab.runtime_state == TabRuntimeState::Restoring {
                tab.runtime_state = restored_state;
                ops.push(PatchOp::UpsertTab(tab.clone()));
            }
        }
        Intent::NewProfile { name } => {
            let profile_id = state.add_profile(name);
            let workspace_id = state
                .add_workspace(profile_id, "Workspace 1")
                .map_err(|_| ReduceError::ProfileNotFound(profile_id))?;
            state.active_profile_id = Some(profile_id);
            ops.push(PatchOp::UpsertProfile(state.profiles[&profile_id].clone()));
            ops.push(PatchOp::UpsertWorkspace(
                state.workspaces[&workspace_id].clone(),
            ));
            ops.push(PatchOp::SetActiveProfile { profile_id });
            ops.push(PatchOp::SetActiveWorkspace {
                profile_id,
                workspace_id,
            });
            enforce = true;
        }
        Intent::RenameProfile { profile_id, name } => {
            let profile = state
                .profiles
                .get_mut(&profile_id)
                .ok_or(ReduceError::ProfileNotFound(profile_id))?;
            profile.name = name;
            ops.push(PatchOp::UpsertProfile(profile.clone()));
        }
        Intent::DeleteProfile { profile_id } => {
            if !state.profiles.contains_key(&profile_id) {
                return Err(ReduceError::ProfileNotFound(profile_id));
            }
            if state.profiles.len() == 1 {
                return Err(ReduceError::CannotDeleteLastProfile(profile_id));
            }
            let workspace_ids = state.profiles[&profile_id].workspace_order.clone();
            for workspace_id in workspace_ids {
                let tab_ids: Vec<_> = state
                    .tabs
                    .values()
                    .filter(|tab| tab.workspace_id == workspace_id)
                    .map(|tab| tab.id)
                    .collect();
                for tab_id in tab_ids {
                    state.tabs.remove(&tab_id);
                    ops.push(PatchOp::RemoveTab {
                        tab_id,
                        workspace_id,
                    });
                }
                state.workspaces.remove(&workspace_id);
                ops.push(PatchOp::RemoveWorkspace {
                    workspace_id,
                    profile_id,
                });
            }
            state.profiles.remove(&profile_id);
            state.history.remove(&profile_id);
            state.warm_lru.remove(&profile_id);
            ops.push(PatchOp::RemoveProfile { profile_id });
            if state.active_profile_id == Some(profile_id) {
                let next = state.profiles.keys().next().copied();
                state.active_profile_id = next;
                if let Some(next_profile_id) = next {
                    ops.push(PatchOp::SetActiveProfile {
                        profile_id: next_profile_id,
                    });
                }
            }
            enforce = true;
        }
        Intent::SwitchProfile { profile_id } => {
            let profile = state
                .profiles
                .get(&profile_id)
                .cloned()
                .ok_or(ReduceError::ProfileNotFound(profile_id))?;
            state.active_profile_id = Some(profile_id);
            ops.push(PatchOp::SetActiveProfile { profile_id });
            if let Some(workspace_id) = profile.active_workspace_id {
                ops.push(PatchOp::SetActiveWorkspace {
                    profile_id,
                    workspace_id,
                });
            }
            enforce = true;
        }
        Intent::NewWorkspace { profile_id, name } => {
            let workspace_id = state
                .add_workspace(profile_id, name)
                .map_err(|_| ReduceError::ProfileNotFound(profile_id))?;
            ops.push(PatchOp::UpsertWorkspace(
                state.workspaces[&workspace_id].clone(),
            ));
            ops.push(PatchOp::UpsertProfile(state.profiles[&profile_id].clone()));
        }
        Intent::RenameWorkspace { workspace_id, name } => {
            let workspace = state
                .workspaces
                .get_mut(&workspace_id)
                .ok_or(ReduceError::WorkspaceNotFound(workspace_id))?;
            workspace.name = name;
            ops.push(PatchOp::UpsertWorkspace(workspace.clone()));
        }
        Intent::DeleteWorkspace { workspace_id } => {
            let workspace = state
                .workspaces
                .get(&workspace_id)
                .cloned()
                .ok_or(ReduceError::WorkspaceNotFound(workspace_id))?;
            let profile_id = workspace.profile_id;
            if state.profiles[&profile_id].workspace_order.len() == 1 {
                return Err(ReduceError::CannotDeleteLastWorkspace(workspace_id));
            }
            let tab_ids: Vec<_> = state
                .tabs
                .values()
                .filter(|tab| tab.workspace_id == workspace_id)
                .map(|tab| tab.id)
                .collect();
            for tab_id in tab_ids {
                state.tabs.remove(&tab_id);
                state.remove_from_warm_lru(profile_id, tab_id);
                ops.push(PatchOp::RemoveTab {
                    tab_id,
                    workspace_id,
                });
            }
            state.workspaces.remove(&workspace_id);
            let profile = state
                .profiles
                .get_mut(&profile_id)
                .expect("workspace profile exists");
            profile.workspace_order.retain(|id| *id != workspace_id);
            if profile.active_workspace_id == Some(workspace_id) {
                profile.active_workspace_id = profile.workspace_order.first().copied();
            }
            ops.push(PatchOp::RemoveWorkspace {
                workspace_id,
                profile_id,
            });
            ops.push(PatchOp::UpsertProfile(profile.clone()));
            enforce = true;
        }
        Intent::SwitchWorkspace { workspace_id } => {
            let profile_id = state
                .workspaces
                .get(&workspace_id)
                .ok_or(ReduceError::WorkspaceNotFound(workspace_id))?
                .profile_id;
            state.active_profile_id = Some(profile_id);
            let profile = state
                .profiles
                .get_mut(&profile_id)
                .expect("workspace profile exists");
            profile.active_workspace_id = Some(workspace_id);
            ops.push(PatchOp::UpsertProfile(profile.clone()));
            ops.push(PatchOp::SetActiveProfile { profile_id });
            ops.push(PatchOp::SetActiveWorkspace {
                profile_id,
                workspace_id,
            });
            enforce = true;
        }
        Intent::NewTab {
            workspace_id,
            url,
            make_active,
        } => {
            let resolved = url.unwrap_or_else(|| resolve_new_tab_url(state, workspace_id));
            create_tab(state, workspace_id, None, resolved, make_active, &mut ops)?;
            enforce = true;
        }
        Intent::NewSubtab {
            workspace_id,
            parent_tab_id,
            url,
            make_active,
        } => {
            validate_parent(state, workspace_id, parent_tab_id, None)?;
            create_tab(
                state,
                workspace_id,
                Some(parent_tab_id),
                url,
                make_active,
                &mut ops,
            )?;
            enforce = true;
        }
        Intent::OpenLinkFromTab {
            source_tab_id,
            url,
            secondary,
        } => {
            let source = state
                .tabs
                .get(&source_tab_id)
                .cloned()
                .ok_or(ReduceError::TabNotFound(source_tab_id))?;
            if !source.status.is_open() {
                return Err(ReduceError::TabNotOpen(source_tab_id));
            }
            let tab_id = create_tab(
                state,
                source.workspace_id,
                Some(source_tab_id),
                url,
                false,
                &mut ops,
            )?;
            if secondary {
                let workspace = state
                    .workspaces
                    .get_mut(&source.workspace_id)
                    .expect("source workspace exists");
                workspace.secondary_tab_id = Some(tab_id);
                workspace.split_enabled = true;
                ops.push(PatchOp::UpsertWorkspace(workspace.clone()));
            }
            enforce = true;
        }
        Intent::Navigate { tab_id, url } | Intent::ObserveTabUrl { tab_id, url } => {
            let tab = open_tab_mut(state, tab_id)?;
            if tab.url != url {
                tab.url = url;
                ops.push(PatchOp::UpsertTab(tab.clone()));
            }
        }
        Intent::ObserveTabTitle { tab_id, title } => {
            let tab = state
                .tabs
                .get_mut(&tab_id)
                .ok_or(ReduceError::TabNotFound(tab_id))?;
            if tab.observed_title != title {
                tab.observed_title = title;
                tab.refresh_display_title();
                ops.push(PatchOp::UpsertTab(tab.clone()));
            }
        }
        Intent::RenameTab { tab_id, title } => {
            let tab = state
                .tabs
                .get_mut(&tab_id)
                .ok_or(ReduceError::TabNotFound(tab_id))?;
            let title = title.trim().to_owned();
            tab.custom_title = (!title.is_empty()).then_some(title);
            tab.refresh_display_title();
            ops.push(PatchOp::UpsertTab(tab.clone()));
        }
        Intent::ObserveTabLoading { tab_id, is_loading } => {
            let tab = open_tab_mut(state, tab_id)?;
            if tab.loading != is_loading {
                tab.loading = is_loading;
                ops.push(PatchOp::UpsertTab(tab.clone()));
            }
        }
        Intent::ObserveTabNavigationState {
            tab_id,
            can_go_back,
            can_go_forward,
        } => {
            let tab = open_tab_mut(state, tab_id)?;
            if tab.can_go_back != can_go_back || tab.can_go_forward != can_go_forward {
                tab.can_go_back = can_go_back;
                tab.can_go_forward = can_go_forward;
                ops.push(PatchOp::UpsertTab(tab.clone()));
            }
        }
        Intent::ObserveTabThumbnail { tab_id, .. } => {
            if !state.tabs.contains_key(&tab_id) {
                return Err(ReduceError::TabNotFound(tab_id));
            }
        }
        Intent::ActivateTab { tab_id } => {
            let tab = state
                .tabs
                .get(&tab_id)
                .ok_or(ReduceError::TabNotFound(tab_id))?;
            if !tab.status.is_open() {
                return Err(ReduceError::TabNotOpen(tab_id));
            }
            let already_active = state.active_profile_id == Some(tab.profile_id)
                && state
                    .profiles
                    .get(&tab.profile_id)
                    .is_some_and(|profile| profile.active_workspace_id == Some(tab.workspace_id))
                && state
                    .workspaces
                    .get(&tab.workspace_id)
                    .is_some_and(|workspace| workspace.primary_tab_id == Some(tab_id))
                && tab.runtime_state == TabRuntimeState::Active;
            if !already_active {
                activate_primary(state, tab_id, &mut ops)?;
                enforce = true;
            }
        }
        Intent::CloseTab { tab_id } => {
            complete_tab(state, tab_id, 0, false, &mut ops)?;
            enforce = true;
        }
        Intent::CompleteTab {
            tab_id,
            completed_at_ms,
        } => {
            complete_tab(state, tab_id, completed_at_ms, false, &mut ops)?;
            prune_archive(state, completed_at_ms, &mut ops);
            enforce = true;
        }
        Intent::SnoozeTab { tab_id, wake_at_ms } => {
            complete_tab(state, tab_id, wake_at_ms, true, &mut ops)?;
            enforce = true;
        }
        Intent::RestoreTab { tab_id } => {
            restore_tab(state, tab_id, false, &mut ops)?;
            enforce = true;
        }
        Intent::WakeSnoozed { now_ms } => {
            let due: Vec<_> = state
                .tabs
                .values()
                .filter_map(|tab| match tab.status {
                    TabStatus::Snoozed { wake_at_ms, .. } if wake_at_ms <= now_ms => Some(tab.id),
                    _ => None,
                })
                .collect();
            for tab_id in due {
                restore_tab(state, tab_id, false, &mut ops)?;
            }
            prune_archive(state, now_ms, &mut ops);
            enforce = true;
        }
        Intent::PermanentlyDeleteTab { tab_id } => {
            let tab = state
                .tabs
                .get(&tab_id)
                .cloned()
                .ok_or(ReduceError::TabNotFound(tab_id))?;
            if tab.locked {
                return Err(ReduceError::TabLocked(tab_id));
            }
            delete_tab_record(state, tab_id, &mut ops);
            enforce = true;
        }
        Intent::ClearArchive { profile_id } => {
            if !state.profiles.contains_key(&profile_id) {
                return Err(ReduceError::ProfileNotFound(profile_id));
            }
            let archived: Vec<_> = state
                .tabs
                .values()
                .filter(|tab| {
                    tab.profile_id == profile_id
                        && !tab.locked
                        && matches!(tab.status, TabStatus::Done { .. })
                })
                .map(|tab| tab.id)
                .collect();
            for tab_id in archived {
                delete_tab_record(state, tab_id, &mut ops);
            }
        }
        Intent::MoveTab {
            tab_id,
            workspace_id,
            index,
        } => {
            move_tab_tree(state, tab_id, workspace_id, None, index, &mut ops)?;
            enforce = true;
        }
        Intent::MoveTabTree {
            tab_id,
            workspace_id,
            parent_tab_id,
            sibling_index,
        } => {
            move_tab_tree(
                state,
                tab_id,
                workspace_id,
                parent_tab_id,
                sibling_index,
                &mut ops,
            )?;
            enforce = true;
        }
        Intent::PinTab { tab_id, pinned } => {
            let workspace_id = {
                let tab = open_tab_mut(state, tab_id)?;
                tab.pinned = pinned;
                ops.push(PatchOp::UpsertTab(tab.clone()));
                tab.workspace_id
            };
            rebuild_workspace_order(state, workspace_id);
            ops.push(PatchOp::UpsertWorkspace(
                state.workspaces[&workspace_id].clone(),
            ));
        }
        Intent::SetTabLocked { tab_id, locked } => {
            let tab = state
                .tabs
                .get_mut(&tab_id)
                .ok_or(ReduceError::TabNotFound(tab_id))?;
            tab.locked = locked;
            ops.push(PatchOp::UpsertTab(tab.clone()));
        }
        Intent::DiscardTab { tab_id } => {
            let tab = state
                .tabs
                .get(&tab_id)
                .cloned()
                .ok_or(ReduceError::TabNotFound(tab_id))?;
            if state.visible_tab_ids().contains(&tab_id) {
                return Err(ReduceError::CannotDiscardVisibleTab(tab_id));
            }
            state.remove_from_warm_lru(tab.profile_id, tab_id);
            let tab = state.tabs.get_mut(&tab_id).expect("tab exists");
            tab.runtime_state = TabRuntimeState::Discarded;
            ops.push(PatchOp::UpsertTab(tab.clone()));
        }
        Intent::SetSecondaryTab {
            workspace_id,
            tab_id,
        } => {
            let tab = state
                .tabs
                .get(&tab_id)
                .ok_or(ReduceError::TabNotFound(tab_id))?;
            if tab.workspace_id != workspace_id || !tab.status.is_open() {
                return Err(ReduceError::InvalidSplitTab {
                    workspace_id,
                    tab_id,
                });
            }
            let workspace = state
                .workspaces
                .get_mut(&workspace_id)
                .ok_or(ReduceError::WorkspaceNotFound(workspace_id))?;
            if workspace.primary_tab_id == Some(tab_id) {
                return Err(ReduceError::InvalidSplitTab {
                    workspace_id,
                    tab_id,
                });
            }
            workspace.secondary_tab_id = Some(tab_id);
            workspace.split_enabled = true;
            ops.push(PatchOp::UpsertWorkspace(workspace.clone()));
            enforce = true;
        }
        Intent::ClearSplit { workspace_id } => {
            let workspace = state
                .workspaces
                .get_mut(&workspace_id)
                .ok_or(ReduceError::WorkspaceNotFound(workspace_id))?;
            workspace.split_enabled = false;
            ops.push(PatchOp::UpsertWorkspace(workspace.clone()));
            enforce = true;
        }
        Intent::SwapSplit { workspace_id } => {
            let workspace = state
                .workspaces
                .get_mut(&workspace_id)
                .ok_or(ReduceError::WorkspaceNotFound(workspace_id))?;
            if workspace.split_enabled {
                if let Some(secondary) = workspace.secondary_tab_id {
                    let primary = workspace.primary_tab_id;
                    workspace.set_primary(Some(secondary));
                    workspace.secondary_tab_id = primary;
                    ops.push(PatchOp::UpsertWorkspace(workspace.clone()));
                    ops.push(PatchOp::SetActiveTab {
                        workspace_id,
                        tab_id: Some(secondary),
                    });
                }
            }
            enforce = true;
        }
        Intent::SetSplitRatio {
            workspace_id,
            ratio,
        } => {
            if !ratio.is_finite() {
                return Err(ReduceError::InvalidSplitRatio);
            }
            let workspace = state
                .workspaces
                .get_mut(&workspace_id)
                .ok_or(ReduceError::WorkspaceNotFound(workspace_id))?;
            workspace.split_ratio = ratio.clamp(0.25, 0.75);
            ops.push(PatchOp::UpsertWorkspace(workspace.clone()));
        }
        Intent::RecordHistory {
            profile_id,
            url,
            title,
            visited_at_ms,
        } => {
            if !state.profiles.contains_key(&profile_id) {
                return Err(ReduceError::ProfileNotFound(profile_id));
            }
            let entries = state.history.entry(profile_id).or_default();
            if let Some(last) = entries
                .last_mut()
                .filter(|entry| normalize_url(&entry.url) == normalize_url(&url))
            {
                last.visit_count = last.visit_count.saturating_add(1);
                last.last_visit_ms = visited_at_ms;
                if !title.trim().is_empty() {
                    last.title = title;
                }
            } else {
                entries.push(HistoryEntry {
                    profile_id,
                    url,
                    title,
                    visit_count: 1,
                    last_visit_ms: visited_at_ms,
                });
            }
            ops.push(PatchOp::HistoryChanged {
                profile_id: Some(profile_id),
            });
        }
        Intent::ClearHistory { scope } => {
            let profile_id = match scope {
                ClearHistoryScope::Profile(profile_id) => {
                    if !state.profiles.contains_key(&profile_id) {
                        return Err(ReduceError::ProfileNotFound(profile_id));
                    }
                    state.history.remove(&profile_id);
                    Some(profile_id)
                }
                ClearHistoryScope::All => {
                    state.history.clear();
                    None
                }
            };
            ops.push(PatchOp::HistoryChanged { profile_id });
        }
        Intent::SettingSet { key, value } => {
            if key.starts_with("keybinding_") {
                if let SettingValue::Text(candidate) = &value {
                    let duplicate = state.settings.iter().any(|(other_key, other_value)| {
                        other_key != &key
                            && other_key.starts_with("keybinding_")
                            && matches!(other_value, SettingValue::Text(existing) if existing.eq_ignore_ascii_case(candidate))
                    });
                    if duplicate {
                        return Err(ReduceError::AmbiguousKeybinding(candidate.clone()));
                    }
                }
            }
            if key == WARM_POOL_BUDGET_KEY {
                enforce = true;
            }
            state.settings.insert(key.clone(), value.clone());
            ops.push(PatchOp::SettingChanged { key, value });
        }
    }

    if enforce {
        enforce_lifecycle_policy(state, &mut ops);
    }
    Ok(deduplicate_upserts(ops))
}

fn create_tab(
    state: &mut BrowserState,
    workspace_id: WorkspaceId,
    parent_tab_id: Option<TabId>,
    url: String,
    make_active: bool,
    ops: &mut Vec<PatchOp>,
) -> Result<TabId, ReduceError> {
    let profile_id = state
        .workspaces
        .get(&workspace_id)
        .ok_or(ReduceError::WorkspaceNotFound(workspace_id))?
        .profile_id;
    if let Some(parent_id) = parent_tab_id {
        validate_parent(state, workspace_id, parent_id, None)?;
    }
    let tab_id = state.allocate_tab_id();
    state.tabs.insert(
        tab_id,
        Tab {
            id: tab_id,
            profile_id,
            workspace_id,
            parent_tab_id,
            url,
            title: String::new(),
            observed_title: String::new(),
            custom_title: None,
            pinned: false,
            locked: false,
            muted: false,
            status: TabStatus::Open,
            loading: false,
            can_go_back: false,
            can_go_forward: false,
            runtime_state: TabRuntimeState::Discarded,
        },
    );
    let sibling_count = open_siblings(state, workspace_id, parent_tab_id)
        .into_iter()
        .filter(|id| *id != tab_id)
        .count();
    insert_subtree_at(state, workspace_id, tab_id, parent_tab_id, sibling_count);
    if make_active {
        activate_primary(state, tab_id, ops)?;
    }
    ops.push(PatchOp::UpsertWorkspace(
        state.workspaces[&workspace_id].clone(),
    ));
    ops.push(PatchOp::UpsertTab(state.tabs[&tab_id].clone()));
    Ok(tab_id)
}

fn activate_primary(
    state: &mut BrowserState,
    tab_id: TabId,
    ops: &mut Vec<PatchOp>,
) -> Result<(), ReduceError> {
    let tab = state
        .tabs
        .get(&tab_id)
        .ok_or(ReduceError::TabNotFound(tab_id))?;
    if !tab.status.is_open() {
        return Err(ReduceError::TabNotOpen(tab_id));
    }
    let workspace_id = tab.workspace_id;
    let profile_id = tab.profile_id;
    let workspace = state
        .workspaces
        .get_mut(&workspace_id)
        .expect("tab workspace exists");
    if workspace.secondary_tab_id == Some(tab_id) {
        workspace.secondary_tab_id = workspace.primary_tab_id;
    }
    workspace.set_primary(Some(tab_id));
    ops.push(PatchOp::UpsertWorkspace(workspace.clone()));
    ops.push(PatchOp::SetActiveTab {
        workspace_id,
        tab_id: Some(tab_id),
    });
    let profile = state
        .profiles
        .get_mut(&profile_id)
        .expect("tab profile exists");
    profile.active_workspace_id = Some(workspace_id);
    ops.push(PatchOp::UpsertProfile(profile.clone()));
    state.active_profile_id = Some(profile_id);
    ops.push(PatchOp::SetActiveProfile { profile_id });
    ops.push(PatchOp::SetActiveWorkspace {
        profile_id,
        workspace_id,
    });
    Ok(())
}

fn complete_tab(
    state: &mut BrowserState,
    tab_id: TabId,
    timestamp_ms: i64,
    snoozed: bool,
    ops: &mut Vec<PatchOp>,
) -> Result<(), ReduceError> {
    let tab = state
        .tabs
        .get(&tab_id)
        .cloned()
        .ok_or(ReduceError::TabNotFound(tab_id))?;
    if tab.locked {
        return Err(ReduceError::TabLocked(tab_id));
    }
    if !tab.status.is_open() {
        return Err(ReduceError::TabNotOpen(tab_id));
    }
    let workspace_id = tab.workspace_id;
    let siblings = open_siblings(state, workspace_id, tab.parent_tab_id);
    let sibling_index = siblings
        .iter()
        .position(|id| *id == tab_id)
        .unwrap_or(siblings.len());
    let fallback = next_fallback_tab(state, &tab, &siblings, sibling_index);

    // Open direct children are promoted to the completed tab's parent.
    let children: Vec<_> = state
        .workspaces
        .get(&workspace_id)
        .expect("workspace exists")
        .tab_order
        .iter()
        .copied()
        .filter(|id| {
            state
                .tabs
                .get(id)
                .is_some_and(|child| child.parent_tab_id == Some(tab_id))
        })
        .collect();
    for child_id in children {
        if let Some(child) = state.tabs.get_mut(&child_id) {
            child.parent_tab_id = tab.parent_tab_id;
            ops.push(PatchOp::UpsertTab(child.clone()));
        }
    }

    let restore = RestorePosition {
        parent_tab_id: tab.parent_tab_id,
        sibling_index,
    };
    let archived = state.tabs.get_mut(&tab_id).expect("tab exists");
    archived.status = if snoozed {
        TabStatus::Snoozed {
            wake_at_ms: timestamp_ms,
            restore,
        }
    } else {
        TabStatus::Done {
            completed_at_ms: timestamp_ms,
            restore,
        }
    };
    archived.runtime_state = TabRuntimeState::Discarded;
    ops.push(PatchOp::UpsertTab(archived.clone()));
    state.remove_from_warm_lru(tab.profile_id, tab_id);

    let workspace = state
        .workspaces
        .get_mut(&workspace_id)
        .expect("workspace exists");
    workspace.tab_order.retain(|id| *id != tab_id);
    if workspace.secondary_tab_id == Some(tab_id) {
        workspace.secondary_tab_id = None;
        workspace.split_enabled = false;
    }
    if workspace.primary_tab_id == Some(tab_id) {
        workspace.set_primary(fallback);
        ops.push(PatchOp::SetActiveTab {
            workspace_id,
            tab_id: fallback,
        });
    }
    rebuild_workspace_order(state, workspace_id);
    ops.push(PatchOp::UpsertWorkspace(
        state.workspaces[&workspace_id].clone(),
    ));
    Ok(())
}

fn restore_tab(
    state: &mut BrowserState,
    tab_id: TabId,
    make_active: bool,
    ops: &mut Vec<PatchOp>,
) -> Result<(), ReduceError> {
    let tab = state
        .tabs
        .get(&tab_id)
        .cloned()
        .ok_or(ReduceError::TabNotFound(tab_id))?;
    let restore = match tab.status {
        TabStatus::Done { restore, .. } | TabStatus::Snoozed { restore, .. } => restore,
        TabStatus::Open => return Err(ReduceError::TabNotOpen(tab_id)),
    };
    let parent = restore.parent_tab_id.filter(|parent_id| {
        state.tabs.get(parent_id).is_some_and(|parent| {
            parent.workspace_id == tab.workspace_id && parent.status.is_open()
        })
    });
    let restored = state.tabs.get_mut(&tab_id).expect("tab exists");
    restored.parent_tab_id = parent;
    restored.status = TabStatus::Open;
    insert_subtree_at(
        state,
        tab.workspace_id,
        tab_id,
        parent,
        restore.sibling_index,
    );
    ops.push(PatchOp::UpsertTab(state.tabs[&tab_id].clone()));
    ops.push(PatchOp::UpsertWorkspace(
        state.workspaces[&tab.workspace_id].clone(),
    ));
    if make_active {
        activate_primary(state, tab_id, ops)?;
    }
    Ok(())
}

fn move_tab_tree(
    state: &mut BrowserState,
    tab_id: TabId,
    target_workspace_id: WorkspaceId,
    parent_tab_id: Option<TabId>,
    sibling_index: usize,
    ops: &mut Vec<PatchOp>,
) -> Result<(), ReduceError> {
    let tab = state
        .tabs
        .get(&tab_id)
        .cloned()
        .ok_or(ReduceError::TabNotFound(tab_id))?;
    if !tab.status.is_open() {
        return Err(ReduceError::TabNotOpen(tab_id));
    }
    let target_profile = state
        .workspaces
        .get(&target_workspace_id)
        .ok_or(ReduceError::WorkspaceNotFound(target_workspace_id))?
        .profile_id;
    if target_profile != tab.profile_id {
        return Err(ReduceError::CrossProfileMove {
            tab_id,
            from_profile: tab.profile_id,
            to_profile: target_profile,
        });
    }
    if let Some(parent_id) = parent_tab_id {
        validate_parent(state, target_workspace_id, parent_id, Some(tab_id))?;
    }
    let subtree = subtree_ids(state, tab_id);
    if parent_tab_id.is_some_and(|parent| subtree.contains(&parent)) {
        return Err(ReduceError::TreeCycle {
            tab_id,
            parent_tab_id: parent_tab_id.unwrap(),
        });
    }
    let target_siblings: Vec<_> = open_siblings(state, target_workspace_id, parent_tab_id)
        .into_iter()
        .filter(|candidate| !subtree.contains(candidate))
        .collect();
    if sibling_index > target_siblings.len() {
        return Err(ReduceError::InvalidSiblingPosition {
            tab_id,
            sibling_index,
        });
    }
    let pinned_count = target_siblings
        .iter()
        .filter(|candidate| state.tabs[candidate].pinned)
        .count();
    if (tab.pinned && sibling_index > pinned_count) || (!tab.pinned && sibling_index < pinned_count)
    {
        return Err(ReduceError::PinnedOrderViolation {
            tab_id,
            sibling_index,
        });
    }
    let source_workspace_id = tab.workspace_id;
    if let Some(source) = state.workspaces.get_mut(&source_workspace_id) {
        source.tab_order.retain(|id| !subtree.contains(id));
        if source
            .secondary_tab_id
            .is_some_and(|id| subtree.contains(&id))
        {
            source.secondary_tab_id = None;
            source.split_enabled = false;
        }
        if source
            .primary_tab_id
            .is_some_and(|id| subtree.contains(&id))
        {
            let next = source.tab_order.first().copied();
            source.set_primary(next);
        }
    }
    for moved_id in &subtree {
        let moved = state.tabs.get_mut(moved_id).expect("subtree tab exists");
        moved.workspace_id = target_workspace_id;
        moved.profile_id = target_profile;
        if *moved_id == tab_id {
            moved.parent_tab_id = parent_tab_id;
        }
        ops.push(PatchOp::UpsertTab(moved.clone()));
    }
    insert_tab_ids_at(
        state,
        target_workspace_id,
        &subtree,
        parent_tab_id,
        sibling_index,
    );
    rebuild_workspace_order(state, source_workspace_id);
    rebuild_workspace_order(state, target_workspace_id);
    ops.push(PatchOp::UpsertWorkspace(
        state.workspaces[&source_workspace_id].clone(),
    ));
    if target_workspace_id != source_workspace_id {
        ops.push(PatchOp::UpsertWorkspace(
            state.workspaces[&target_workspace_id].clone(),
        ));
    }
    Ok(())
}

fn validate_parent(
    state: &BrowserState,
    workspace_id: WorkspaceId,
    parent_tab_id: TabId,
    moving_tab_id: Option<TabId>,
) -> Result<(), ReduceError> {
    let parent = state
        .tabs
        .get(&parent_tab_id)
        .ok_or(ReduceError::InvalidParent {
            tab_id: moving_tab_id.unwrap_or(parent_tab_id),
            parent_tab_id,
        })?;
    if parent.workspace_id != workspace_id || !parent.status.is_open() {
        return Err(ReduceError::InvalidParent {
            tab_id: moving_tab_id.unwrap_or(parent_tab_id),
            parent_tab_id,
        });
    }
    Ok(())
}

fn open_tab_mut(state: &mut BrowserState, tab_id: TabId) -> Result<&mut Tab, ReduceError> {
    let tab = state
        .tabs
        .get_mut(&tab_id)
        .ok_or(ReduceError::TabNotFound(tab_id))?;
    if !tab.status.is_open() {
        return Err(ReduceError::TabNotOpen(tab_id));
    }
    Ok(tab)
}

fn open_siblings(
    state: &BrowserState,
    workspace_id: WorkspaceId,
    parent_tab_id: Option<TabId>,
) -> Vec<TabId> {
    state
        .workspaces
        .get(&workspace_id)
        .map(|workspace| {
            workspace
                .tab_order
                .iter()
                .copied()
                .filter(|id| {
                    state.tabs.get(id).is_some_and(|tab| {
                        tab.status.is_open() && tab.parent_tab_id == parent_tab_id
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn subtree_ids(state: &BrowserState, root: TabId) -> Vec<TabId> {
    let Some(root_tab) = state.tabs.get(&root) else {
        return Vec::new();
    };
    let Some(workspace) = state.workspaces.get(&root_tab.workspace_id) else {
        return vec![root];
    };
    workspace
        .tab_order
        .iter()
        .copied()
        .filter(|candidate| *candidate == root || is_descendant(state, *candidate, root))
        .collect()
}

fn is_descendant(state: &BrowserState, mut candidate: TabId, ancestor: TabId) -> bool {
    let mut seen = BTreeSet::new();
    while let Some(parent) = state.tabs.get(&candidate).and_then(|tab| tab.parent_tab_id) {
        if parent == ancestor {
            return true;
        }
        if !seen.insert(parent) {
            return false;
        }
        candidate = parent;
    }
    false
}

fn insert_subtree_at(
    state: &mut BrowserState,
    workspace_id: WorkspaceId,
    tab_id: TabId,
    parent_tab_id: Option<TabId>,
    sibling_index: usize,
) {
    let subtree = subtree_ids(state, tab_id);
    let subtree = if subtree.is_empty() {
        vec![tab_id]
    } else {
        subtree
    };
    insert_tab_ids_at(state, workspace_id, &subtree, parent_tab_id, sibling_index);
}

fn insert_tab_ids_at(
    state: &mut BrowserState,
    workspace_id: WorkspaceId,
    subtree: &[TabId],
    parent_tab_id: Option<TabId>,
    sibling_index: usize,
) {
    let mut order = state
        .workspaces
        .get(&workspace_id)
        .expect("workspace exists")
        .tab_order
        .clone();
    order.retain(|id| !subtree.contains(id));
    let siblings: Vec<_> = order
        .iter()
        .copied()
        .filter(|id| {
            state
                .tabs
                .get(id)
                .is_some_and(|tab| tab.parent_tab_id == parent_tab_id)
        })
        .collect();
    let insert_at = if let Some(before) = siblings.get(sibling_index) {
        order
            .iter()
            .position(|id| id == before)
            .unwrap_or(order.len())
    } else if let Some(last) = siblings.last() {
        let last_index = order
            .iter()
            .position(|id| id == last)
            .unwrap_or(order.len());
        let descendants_after = order[last_index.saturating_add(1)..]
            .iter()
            .take_while(|candidate| is_descendant(state, **candidate, *last))
            .count();
        last_index + 1 + descendants_after
    } else if let Some(parent) = parent_tab_id {
        order
            .iter()
            .position(|id| *id == parent)
            .map(|i| i + 1)
            .unwrap_or(order.len())
    } else {
        order.len()
    };
    order.splice(insert_at..insert_at, subtree.iter().copied());
    state
        .workspaces
        .get_mut(&workspace_id)
        .expect("workspace exists")
        .tab_order = order;
    rebuild_workspace_order(state, workspace_id);
}

fn rebuild_workspace_order(state: &mut BrowserState, workspace_id: WorkspaceId) {
    let Some(workspace) = state.workspaces.get(&workspace_id) else {
        return;
    };
    let existing = workspace.tab_order.clone();
    let mut by_parent: BTreeMap<Option<TabId>, Vec<TabId>> = BTreeMap::new();
    for tab_id in existing {
        if let Some(tab) = state.tabs.get(&tab_id) {
            if tab.workspace_id == workspace_id && tab.status.is_open() {
                let valid_parent = tab.parent_tab_id.filter(|parent| {
                    state.tabs.get(parent).is_some_and(|candidate| {
                        candidate.workspace_id == workspace_id && candidate.status.is_open()
                    })
                });
                by_parent.entry(valid_parent).or_default().push(tab_id);
            }
        }
    }
    for siblings in by_parent.values_mut() {
        siblings.sort_by_key(|id| !state.tabs[id].pinned);
    }
    fn append(
        parent: Option<TabId>,
        map: &BTreeMap<Option<TabId>, Vec<TabId>>,
        out: &mut Vec<TabId>,
    ) {
        if let Some(children) = map.get(&parent) {
            for child in children {
                out.push(*child);
                append(Some(*child), map, out);
            }
        }
    }
    let mut rebuilt = Vec::new();
    append(None, &by_parent, &mut rebuilt);
    // Corrupt/orphaned nodes fail safe to root instead of disappearing.
    for tab_id in by_parent.values().flatten() {
        if !rebuilt.contains(tab_id) {
            rebuilt.push(*tab_id);
        }
    }
    if let Some(workspace) = state.workspaces.get_mut(&workspace_id) {
        workspace.tab_order = rebuilt;
        if workspace
            .primary_tab_id
            .is_some_and(|id| !workspace.tab_order.contains(&id))
        {
            workspace.set_primary(workspace.tab_order.first().copied());
        }
        if workspace
            .secondary_tab_id
            .is_some_and(|id| !workspace.tab_order.contains(&id))
        {
            workspace.secondary_tab_id = None;
            workspace.split_enabled = false;
        }
    }
}

fn next_fallback_tab(
    state: &BrowserState,
    tab: &Tab,
    siblings: &[TabId],
    index: usize,
) -> Option<TabId> {
    siblings
        .get(index + 1)
        .or_else(|| index.checked_sub(1).and_then(|i| siblings.get(i)))
        .copied()
        .or(tab.parent_tab_id)
        .filter(|id| {
            state
                .tabs
                .get(id)
                .is_some_and(|candidate| candidate.status.is_open())
        })
        .or_else(|| {
            state
                .workspaces
                .get(&tab.workspace_id)?
                .tab_order
                .iter()
                .copied()
                .find(|id| *id != tab.id)
        })
}

fn delete_tab_record(state: &mut BrowserState, tab_id: TabId, ops: &mut Vec<PatchOp>) {
    let Some(tab) = state.tabs.remove(&tab_id) else {
        return;
    };
    state.remove_from_warm_lru(tab.profile_id, tab_id);
    for child in state
        .tabs
        .values_mut()
        .filter(|candidate| candidate.parent_tab_id == Some(tab_id))
    {
        child.parent_tab_id = None;
        ops.push(PatchOp::UpsertTab(child.clone()));
    }
    if let Some(workspace) = state.workspaces.get_mut(&tab.workspace_id) {
        workspace.tab_order.retain(|id| *id != tab_id);
        if workspace.primary_tab_id == Some(tab_id) {
            workspace.set_primary(workspace.tab_order.first().copied());
        }
        if workspace.secondary_tab_id == Some(tab_id) {
            workspace.secondary_tab_id = None;
            workspace.split_enabled = false;
        }
        ops.push(PatchOp::UpsertWorkspace(workspace.clone()));
    }
    ops.push(PatchOp::RemoveTab {
        tab_id,
        workspace_id: tab.workspace_id,
    });
}

fn prune_archive(state: &mut BrowserState, now_ms: i64, ops: &mut Vec<PatchOp>) {
    let profile_ids: Vec<_> = state.profiles.keys().copied().collect();
    for profile_id in profile_ids {
        let mut done: Vec<_> = state
            .tabs
            .values()
            .filter_map(|tab| match tab.status {
                TabStatus::Done {
                    completed_at_ms, ..
                } if tab.profile_id == profile_id => Some((tab.id, completed_at_ms)),
                _ => None,
            })
            .collect();
        done.sort_by_key(|(_, completed)| std::cmp::Reverse(*completed));
        let remove: Vec<_> = done
            .iter()
            .enumerate()
            .filter(|(index, (_, completed))| {
                *index >= ARCHIVE_MAX_ENTRIES_PER_PROFILE
                    || (now_ms > 0
                        && *completed > 0
                        && now_ms.saturating_sub(*completed) > ARCHIVE_MAX_AGE_MS)
            })
            .map(|(_, (id, _))| *id)
            .collect();
        for tab_id in remove {
            delete_tab_record(state, tab_id, ops);
        }
    }
}

fn enforce_lifecycle_policy(state: &mut BrowserState, ops: &mut Vec<PatchOp>) {
    state.prune_warm_lru();
    let active_profile = state.active_profile_id;
    let visible = state.visible_tab_ids();
    if let Some(profile_id) = active_profile {
        for tab_id in &visible {
            state.touch_warm_lru(profile_id, *tab_id);
        }
    }
    let warm_budget = warm_pool_budget(state);
    let warm: BTreeSet<_> = active_profile
        .and_then(|profile| state.warm_lru.get(&profile))
        .map(|lru| {
            lru.iter()
                .rev()
                .copied()
                .filter(|id| !visible.contains(id))
                .filter(|id| state.tabs.get(id).is_some_and(|tab| tab.status.is_open()))
                .take(warm_budget)
                .collect()
        })
        .unwrap_or_default();

    let active_workspace = state.active_workspace_id();
    let secondary = active_workspace
        .and_then(|id| state.workspaces.get(&id))
        .filter(|workspace| workspace.split_enabled)
        .and_then(|workspace| workspace.secondary_tab_id);
    let primary = active_workspace
        .and_then(|id| state.workspaces.get(&id))
        .and_then(|workspace| workspace.primary_tab_id);

    for tab in state.tabs.values_mut() {
        let desired = if !tab.status.is_open() {
            TabRuntimeState::Discarded
        } else if Some(tab.profile_id) != active_profile {
            TabRuntimeState::Discarded
        } else if Some(tab.id) == primary {
            match tab.runtime_state {
                TabRuntimeState::Discarded => TabRuntimeState::Restoring,
                TabRuntimeState::Restoring => TabRuntimeState::Restoring,
                _ => TabRuntimeState::Active,
            }
        } else if Some(tab.id) == secondary {
            match tab.runtime_state {
                TabRuntimeState::Discarded => TabRuntimeState::Restoring,
                TabRuntimeState::Restoring => TabRuntimeState::Restoring,
                _ => TabRuntimeState::Secondary,
            }
        } else if warm.contains(&tab.id) {
            TabRuntimeState::Warm
        } else {
            TabRuntimeState::Discarded
        };
        if tab.runtime_state != desired {
            tab.runtime_state = desired;
            ops.push(PatchOp::UpsertTab(tab.clone()));
        }
    }
}

fn warm_pool_budget(state: &BrowserState) -> usize {
    match state.settings.get(WARM_POOL_BUDGET_KEY) {
        Some(SettingValue::Int(value)) => (*value).clamp(0, MAX_WARM_POOL_BUDGET as i64) as usize,
        _ => DEFAULT_WARM_POOL_BUDGET,
    }
}

fn resolve_new_tab_url(state: &BrowserState, workspace_id: WorkspaceId) -> String {
    match setting_text(state, NEW_TAB_BEHAVIOR_KEY)
        .unwrap_or("homepage")
        .to_ascii_lowercase()
        .as_str()
    {
        "blank" => "about:blank".to_owned(),
        "custom" => setting_text(state, NEW_TAB_CUSTOM_URL_KEY)
            .and_then(normalize_configured_url)
            .unwrap_or_else(|| resolve_homepage_url(state)),
        "workspace_default" => state
            .workspaces
            .get(&workspace_id)
            .and_then(|workspace| workspace.primary_tab_id)
            .and_then(|tab_id| state.tabs.get(&tab_id))
            .map(|tab| tab.url.clone())
            .filter(|url| !url.trim().is_empty())
            .unwrap_or_else(|| resolve_homepage_url(state)),
        _ => resolve_homepage_url(state),
    }
}

fn resolve_homepage_url(state: &BrowserState) -> String {
    setting_text(state, HOMEPAGE_KEY)
        .and_then(normalize_configured_url)
        .unwrap_or_else(|| "about:blank".to_owned())
}

fn setting_text<'a>(state: &'a BrowserState, key: &str) -> Option<&'a str> {
    match state.settings.get(key) {
        Some(SettingValue::Text(value)) => Some(value),
        _ => None,
    }
}

fn normalize_configured_url(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.eq_ignore_ascii_case("about:blank")
        || trimmed.starts_with("https://")
        || trimmed.starts_with("http://")
    {
        return Some(trimmed.to_owned());
    }
    if trimmed.contains("://") {
        return None;
    }
    Some(format!("https://{trimmed}"))
}

fn normalize_url(value: &str) -> String {
    value.trim().trim_end_matches('/').to_ascii_lowercase()
}

fn deduplicate_upserts(ops: Vec<PatchOp>) -> Vec<PatchOp> {
    let mut last_profile = BTreeMap::new();
    let mut last_workspace = BTreeMap::new();
    let mut last_tab = BTreeMap::new();
    let mut passthrough = Vec::new();
    for op in ops {
        match op {
            PatchOp::UpsertProfile(value) => {
                last_profile.insert(value.id, value);
            }
            PatchOp::UpsertWorkspace(value) => {
                last_workspace.insert(value.id, value);
            }
            PatchOp::UpsertTab(value) => {
                last_tab.insert(value.id, value);
            }
            other => passthrough.push(other),
        }
    }
    passthrough.extend(last_profile.into_values().map(PatchOp::UpsertProfile));
    passthrough.extend(last_workspace.into_values().map(PatchOp::UpsertWorkspace));
    passthrough.extend(last_tab.into_values().map(PatchOp::UpsertTab));
    passthrough
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> (BrowserState, ProfileId, WorkspaceId) {
        let mut state = BrowserState::default();
        let profile = state.add_profile("Default");
        let workspace = state.add_workspace(profile, "Main").unwrap();
        (state, profile, workspace)
    }

    fn new_tab(state: &mut BrowserState, workspace: WorkspaceId, url: &str) -> TabId {
        apply_intent(
            state,
            Intent::NewTab {
                workspace_id: workspace,
                url: Some(url.into()),
                make_active: true,
            },
        )
        .unwrap();
        let tab_id = state.workspaces[&workspace].primary_tab_id.unwrap();
        apply_intent(
            state,
            Intent::TabFrameCommitted {
                tab_id,
                generation: 1,
            },
        )
        .unwrap();
        tab_id
    }

    #[test]
    fn complete_and_restore_preserves_position() {
        let (mut state, _, workspace) = state();
        let first = new_tab(&mut state, workspace, "https://one.example");
        let second = new_tab(&mut state, workspace, "https://two.example");
        apply_intent(
            &mut state,
            Intent::CompleteTab {
                tab_id: first,
                completed_at_ms: 100,
            },
        )
        .unwrap();
        assert!(matches!(state.tabs[&first].status, TabStatus::Done { .. }));
        assert_eq!(state.workspaces[&workspace].primary_tab_id, Some(second));
        apply_intent(&mut state, Intent::RestoreTab { tab_id: first }).unwrap();
        assert_eq!(state.workspaces[&workspace].tab_order, vec![first, second]);
    }

    #[test]
    fn tree_move_rejects_cycle() {
        let (mut state, _, workspace) = state();
        let parent = new_tab(&mut state, workspace, "https://parent.example");
        apply_intent(
            &mut state,
            Intent::NewSubtab {
                workspace_id: workspace,
                parent_tab_id: parent,
                url: "https://child.example".into(),
                make_active: false,
            },
        )
        .unwrap();
        let child = state.workspaces[&workspace].tab_order[1];
        let error = apply_intent(
            &mut state,
            Intent::MoveTabTree {
                tab_id: parent,
                workspace_id: workspace,
                parent_tab_id: Some(child),
                sibling_index: 0,
            },
        )
        .unwrap_err();
        assert!(matches!(error, ReduceError::TreeCycle { .. }));
    }

    #[test]
    fn pinned_siblings_sort_before_unpinned_without_moving_subtree() {
        let (mut state, _, workspace) = state();
        let first = new_tab(&mut state, workspace, "https://one.example");
        apply_intent(
            &mut state,
            Intent::NewSubtab {
                workspace_id: workspace,
                parent_tab_id: first,
                url: "https://child.example".into(),
                make_active: false,
            },
        )
        .unwrap();
        let child = state.workspaces[&workspace].tab_order[1];
        let second = new_tab(&mut state, workspace, "https://two.example");
        apply_intent(
            &mut state,
            Intent::PinTab {
                tab_id: second,
                pinned: true,
            },
        )
        .unwrap();
        assert_eq!(
            state.workspaces[&workspace].tab_order,
            vec![second, first, child]
        );
    }

    #[test]
    fn move_rejects_invalid_positions_and_pinned_order_violations() {
        let (mut state, _, workspace) = state();
        let pinned = new_tab(&mut state, workspace, "https://pinned.example");
        apply_intent(
            &mut state,
            Intent::PinTab {
                tab_id: pinned,
                pinned: true,
            },
        )
        .unwrap();
        let ordinary = new_tab(&mut state, workspace, "https://ordinary.example");
        assert_eq!(
            apply_intent(
                &mut state,
                Intent::MoveTabTree {
                    tab_id: ordinary,
                    workspace_id: workspace,
                    parent_tab_id: None,
                    sibling_index: 0,
                },
            ),
            Err(ReduceError::PinnedOrderViolation {
                tab_id: ordinary,
                sibling_index: 0,
            })
        );
        assert_eq!(
            apply_intent(
                &mut state,
                Intent::MoveTabTree {
                    tab_id: ordinary,
                    workspace_id: workspace,
                    parent_tab_id: None,
                    sibling_index: 5,
                },
            ),
            Err(ReduceError::InvalidSiblingPosition {
                tab_id: ordinary,
                sibling_index: 5,
            })
        );
    }

    #[test]
    fn split_has_two_visible_tabs_and_warm_pool_excludes_them() {
        let (mut state, _, workspace) = state();
        let first = new_tab(&mut state, workspace, "https://one.example");
        let second = new_tab(&mut state, workspace, "https://two.example");
        apply_intent(&mut state, Intent::ActivateTab { tab_id: first }).unwrap();
        apply_intent(
            &mut state,
            Intent::SetSecondaryTab {
                workspace_id: workspace,
                tab_id: second,
            },
        )
        .unwrap();
        assert_eq!(state.tabs[&first].runtime_state, TabRuntimeState::Active);
        assert_eq!(
            state.tabs[&second].runtime_state,
            TabRuntimeState::Secondary
        );
    }

    #[test]
    fn modifier_opened_links_become_subtabs_and_shift_target_uses_secondary() {
        let (mut state, _, workspace) = state();
        let source = new_tab(&mut state, workspace, "https://source.example");
        apply_intent(
            &mut state,
            Intent::OpenLinkFromTab {
                source_tab_id: source,
                url: "https://child.example".into(),
                secondary: true,
            },
        )
        .unwrap();

        let workspace_state = &state.workspaces[&workspace];
        let secondary = workspace_state.secondary_tab_id.expect("secondary tab");
        assert!(workspace_state.split_enabled);
        assert_eq!(workspace_state.primary_tab_id, Some(source));
        assert_eq!(state.tabs[&secondary].parent_tab_id, Some(source));
        assert_eq!(
            state.tabs[&secondary].runtime_state,
            TabRuntimeState::Restoring
        );
    }

    #[test]
    fn locked_tab_cannot_be_completed() {
        let (mut state, _, workspace) = state();
        let tab = new_tab(&mut state, workspace, "https://one.example");
        apply_intent(
            &mut state,
            Intent::SetTabLocked {
                tab_id: tab,
                locked: true,
            },
        )
        .unwrap();
        assert_eq!(
            apply_intent(
                &mut state,
                Intent::CompleteTab {
                    tab_id: tab,
                    completed_at_ms: 100
                }
            ),
            Err(ReduceError::TabLocked(tab))
        );
    }

    #[test]
    fn snoozed_tab_wakes_at_saved_position_and_falls_back_when_parent_is_gone() {
        let (mut state, _, workspace) = state();
        let parent = new_tab(&mut state, workspace, "https://parent.example");
        apply_intent(
            &mut state,
            Intent::NewSubtab {
                workspace_id: workspace,
                parent_tab_id: parent,
                url: "https://child.example".into(),
                make_active: false,
            },
        )
        .unwrap();
        let child = state.workspaces[&workspace]
            .tab_order
            .iter()
            .copied()
            .find(|candidate| *candidate != parent)
            .unwrap();
        apply_intent(
            &mut state,
            Intent::SnoozeTab {
                tab_id: child,
                wake_at_ms: 50,
            },
        )
        .unwrap();
        apply_intent(
            &mut state,
            Intent::CompleteTab {
                tab_id: parent,
                completed_at_ms: 10,
            },
        )
        .unwrap();
        apply_intent(&mut state, Intent::PermanentlyDeleteTab { tab_id: parent }).unwrap();
        apply_intent(&mut state, Intent::WakeSnoozed { now_ms: 50 }).unwrap();
        assert!(state.tabs[&child].status.is_open());
        assert_eq!(state.tabs[&child].parent_tab_id, None);
        assert!(state.workspaces[&workspace].tab_order.contains(&child));
    }

    #[test]
    fn completion_selects_next_then_previous_sibling_deterministically() {
        let (mut state, _, workspace) = state();
        let first = new_tab(&mut state, workspace, "https://one.example");
        let second = new_tab(&mut state, workspace, "https://two.example");
        let third = new_tab(&mut state, workspace, "https://three.example");
        apply_intent(&mut state, Intent::ActivateTab { tab_id: second }).unwrap();
        apply_intent(
            &mut state,
            Intent::CompleteTab {
                tab_id: second,
                completed_at_ms: 10,
            },
        )
        .unwrap();
        assert_eq!(state.workspaces[&workspace].primary_tab_id, Some(third));
        apply_intent(
            &mut state,
            Intent::CompleteTab {
                tab_id: third,
                completed_at_ms: 11,
            },
        )
        .unwrap();
        assert_eq!(state.workspaces[&workspace].primary_tab_id, Some(first));
    }

    #[test]
    fn consecutive_history_visits_are_deduplicated() {
        let (mut state, profile, _) = state();
        for at in [10, 20] {
            apply_intent(
                &mut state,
                Intent::RecordHistory {
                    profile_id: profile,
                    url: "https://example.com/".into(),
                    title: "Example".into(),
                    visited_at_ms: at,
                },
            )
            .unwrap();
        }
        assert_eq!(state.history[&profile].len(), 1);
        assert_eq!(state.history[&profile][0].visit_count, 2);
        assert_eq!(state.history[&profile][0].last_visit_ms, 20);
    }

    #[test]
    fn history_clear_respects_profile_and_all_scopes() {
        let (mut state, first_profile, _) = state();
        apply_intent(
            &mut state,
            Intent::NewProfile {
                name: "Other".into(),
            },
        )
        .unwrap();
        let second_profile = state.active_profile_id.unwrap();
        for profile_id in [first_profile, second_profile] {
            apply_intent(
                &mut state,
                Intent::RecordHistory {
                    profile_id,
                    url: format!("https://{}.example", profile_id.0),
                    title: "Visit".into(),
                    visited_at_ms: 1,
                },
            )
            .unwrap();
        }
        apply_intent(
            &mut state,
            Intent::ClearHistory {
                scope: ClearHistoryScope::Profile(first_profile),
            },
        )
        .unwrap();
        assert!(!state.history.contains_key(&first_profile));
        assert!(state.history.contains_key(&second_profile));
        apply_intent(
            &mut state,
            Intent::ClearHistory {
                scope: ClearHistoryScope::All,
            },
        )
        .unwrap();
        assert!(state.history.is_empty());
    }

    #[test]
    fn archive_prunes_by_age_and_profile_cap() {
        let (mut state, profile, workspace) = state();
        let old = new_tab(&mut state, workspace, "https://old.example");
        apply_intent(
            &mut state,
            Intent::CompleteTab {
                tab_id: old,
                completed_at_ms: 1,
            },
        )
        .unwrap();
        apply_intent(
            &mut state,
            Intent::WakeSnoozed {
                now_ms: ARCHIVE_MAX_AGE_MS + 2,
            },
        )
        .unwrap();
        assert!(!state.tabs.contains_key(&old));

        for index in 0..=ARCHIVE_MAX_ENTRIES_PER_PROFILE {
            let tab = new_tab(
                &mut state,
                workspace,
                &format!("https://archive-{index}.example"),
            );
            apply_intent(
                &mut state,
                Intent::CompleteTab {
                    tab_id: tab,
                    completed_at_ms: ARCHIVE_MAX_AGE_MS + 10 + index as i64,
                },
            )
            .unwrap();
        }
        assert_eq!(
            state
                .tabs
                .values()
                .filter(|tab| {
                    tab.profile_id == profile && matches!(tab.status, TabStatus::Done { .. })
                })
                .count(),
            ARCHIVE_MAX_ENTRIES_PER_PROFILE
        );
    }

    #[test]
    fn two_hundred_tabs_respect_visible_and_warm_budgets() {
        let (mut state, first_profile, workspace) = state();
        let mut tabs = Vec::new();
        for index in 0..200 {
            tabs.push(new_tab(
                &mut state,
                workspace,
                &format!("https://tab-{index}.example"),
            ));
        }
        let secondary = tabs[190];
        apply_intent(
            &mut state,
            Intent::SetSecondaryTab {
                workspace_id: workspace,
                tab_id: secondary,
            },
        )
        .unwrap();
        apply_intent(
            &mut state,
            Intent::TabFrameCommitted {
                tab_id: secondary,
                generation: 2,
            },
        )
        .unwrap();
        let first_profile_live = state
            .tabs
            .values()
            .filter(|tab| tab.profile_id == first_profile && tab.runtime_state.is_live())
            .count();
        assert_eq!(first_profile_live, 10, "two visible plus eight warm tabs");

        apply_intent(
            &mut state,
            Intent::NewProfile {
                name: "Other".into(),
            },
        )
        .unwrap();
        assert_eq!(
            state
                .tabs
                .values()
                .filter(|tab| tab.profile_id == first_profile && tab.runtime_state.is_live())
                .count(),
            0,
            "inactive profiles must retain no live views"
        );
    }
}
