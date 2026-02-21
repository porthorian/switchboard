use crate::ids::{ProfileId, TabId, WorkspaceId};
use crate::intent::Intent;
use crate::patch::PatchOp;
use crate::state::{BrowserState, Tab, TabRuntimeState, Workspace};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReduceError {
    ProfileNotFound(ProfileId),
    WorkspaceNotFound(WorkspaceId),
    TabNotFound(TabId),
    CannotDeleteLastWorkspace(WorkspaceId),
    CrossProfileMove {
        tab_id: TabId,
        from_profile: ProfileId,
        to_profile: ProfileId,
    },
    CannotDiscardActiveTab(TabId),
}

pub fn apply_intent(state: &mut BrowserState, intent: Intent) -> Result<Vec<PatchOp>, ReduceError> {
    let mut ops = Vec::new();

    match intent {
        Intent::UiReady { .. } | Intent::FrameCommitted { .. } => {}
        Intent::SwitchProfile { profile_id } => {
            if !state.profiles.contains_key(&profile_id) {
                return Err(ReduceError::ProfileNotFound(profile_id));
            }
            state.active_profile_id = Some(profile_id);
            ops.push(PatchOp::SetActiveProfile { profile_id });
        }
        Intent::NewWorkspace { profile_id, name } => {
            if !state.profiles.contains_key(&profile_id) {
                return Err(ReduceError::ProfileNotFound(profile_id));
            }

            let workspace_id = state.allocate_workspace_id();
            let workspace = Workspace {
                id: workspace_id,
                profile_id,
                name,
                tab_order: Vec::new(),
                active_tab_id: None,
            };
            state.workspaces.insert(workspace_id, workspace.clone());

            let profile_snapshot = {
                let profile = state.profiles.get_mut(&profile_id).expect("checked above");
                profile.workspace_order.push(workspace_id);
                if profile.active_workspace_id.is_none() {
                    profile.active_workspace_id = Some(workspace_id);
                    ops.push(PatchOp::SetActiveWorkspace {
                        profile_id,
                        workspace_id,
                    });
                }
                profile.clone()
            };

            if state.active_profile_id.is_none() {
                state.active_profile_id = Some(profile_id);
                ops.push(PatchOp::SetActiveProfile { profile_id });
            }

            ops.push(PatchOp::UpsertWorkspace(workspace));
            ops.push(PatchOp::UpsertProfile(profile_snapshot));
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

            let can_delete = state
                .profiles
                .get(&profile_id)
                .ok_or(ReduceError::ProfileNotFound(profile_id))?
                .workspace_order
                .len()
                > 1;
            if !can_delete {
                return Err(ReduceError::CannotDeleteLastWorkspace(workspace_id));
            }

            let (profile_snapshot, next_workspace_id, active_workspace_changed) = {
                let profile = state
                    .profiles
                    .get_mut(&profile_id)
                    .ok_or(ReduceError::ProfileNotFound(profile_id))?;
                profile.workspace_order.retain(|id| *id != workspace_id);
                let active_workspace_changed = profile.active_workspace_id == Some(workspace_id);
                if active_workspace_changed {
                    profile.active_workspace_id = profile.workspace_order.first().copied();
                }
                (
                    profile.clone(),
                    profile.active_workspace_id,
                    active_workspace_changed,
                )
            };

            for tab_id in workspace.tab_order {
                if state.tabs.remove(&tab_id).is_some() {
                    ops.push(PatchOp::RemoveTab {
                        tab_id,
                        workspace_id,
                    });
                }
            }
            state.workspaces.remove(&workspace_id);

            ops.push(PatchOp::RemoveWorkspace {
                workspace_id,
                profile_id,
            });
            ops.push(PatchOp::UpsertProfile(profile_snapshot));

            if active_workspace_changed {
                if let Some(next_workspace_id) = next_workspace_id {
                    ops.push(PatchOp::SetActiveWorkspace {
                        profile_id,
                        workspace_id: next_workspace_id,
                    });
                }
                if state.active_profile_id == Some(profile_id) {
                    ops.push(PatchOp::SetActiveProfile { profile_id });
                }
            }
        }
        Intent::SwitchWorkspace { workspace_id } => {
            let profile_id = state
                .workspaces
                .get(&workspace_id)
                .ok_or(ReduceError::WorkspaceNotFound(workspace_id))?
                .profile_id;
            let profile_snapshot = {
                let profile = state
                    .profiles
                    .get_mut(&profile_id)
                    .ok_or(ReduceError::ProfileNotFound(profile_id))?;
                profile.active_workspace_id = Some(workspace_id);
                profile.clone()
            };
            state.active_profile_id = Some(profile_id);

            ops.push(PatchOp::UpsertProfile(profile_snapshot));
            ops.push(PatchOp::SetActiveProfile { profile_id });
            ops.push(PatchOp::SetActiveWorkspace {
                profile_id,
                workspace_id,
            });
        }
        Intent::NewTab {
            workspace_id,
            url,
            make_active,
        } => {
            let profile_id = state
                .workspaces
                .get(&workspace_id)
                .ok_or(ReduceError::WorkspaceNotFound(workspace_id))?
                .profile_id;
            let previous_active_tab = state
                .workspaces
                .get(&workspace_id)
                .and_then(|workspace| workspace.active_tab_id);

            if make_active {
                if let Some(active_tab_id) = previous_active_tab {
                    if let Some(active_tab) = state.tabs.get_mut(&active_tab_id) {
                        active_tab.runtime_state = TabRuntimeState::Warm;
                        ops.push(PatchOp::UpsertTab(active_tab.clone()));
                    }
                }
            }

            let tab_id = state.allocate_tab_id();
            let tab = Tab {
                id: tab_id,
                profile_id,
                workspace_id,
                url: url.unwrap_or_else(|| "about:blank".to_owned()),
                title: String::new(),
                pinned: false,
                muted: false,
                runtime_state: if make_active {
                    TabRuntimeState::Active
                } else {
                    TabRuntimeState::Discarded
                },
            };
            state.tabs.insert(tab_id, tab.clone());

            let workspace = state
                .workspaces
                .get_mut(&workspace_id)
                .ok_or(ReduceError::WorkspaceNotFound(workspace_id))?;
            workspace.tab_order.push(tab_id);
            if make_active {
                workspace.active_tab_id = Some(tab_id);
            }
            ops.push(PatchOp::UpsertWorkspace(workspace.clone()));
            if make_active {
                ops.push(PatchOp::SetActiveTab {
                    workspace_id,
                    tab_id: Some(tab_id),
                });
            }

            if make_active {
                let profile_snapshot = {
                    let profile = state
                        .profiles
                        .get_mut(&profile_id)
                        .ok_or(ReduceError::ProfileNotFound(profile_id))?;
                    profile.active_workspace_id = Some(workspace_id);
                    profile.clone()
                };
                state.active_profile_id = Some(profile_id);
                ops.push(PatchOp::UpsertProfile(profile_snapshot));
                ops.push(PatchOp::SetActiveProfile { profile_id });
                ops.push(PatchOp::SetActiveWorkspace {
                    profile_id,
                    workspace_id,
                });
            }

            ops.push(PatchOp::UpsertTab(tab));
        }
        Intent::Navigate { tab_id, url } => {
            let tab = state
                .tabs
                .get_mut(&tab_id)
                .ok_or(ReduceError::TabNotFound(tab_id))?;
            tab.url = url;
            ops.push(PatchOp::UpsertTab(tab.clone()));
        }
        Intent::PinTab { tab_id, pinned } => {
            let tab = state
                .tabs
                .get_mut(&tab_id)
                .ok_or(ReduceError::TabNotFound(tab_id))?;
            tab.pinned = pinned;
            ops.push(PatchOp::UpsertTab(tab.clone()));
        }
        Intent::ActivateTab { tab_id } => {
            let (workspace_id, profile_id, runtime_state) = {
                let tab = state
                    .tabs
                    .get(&tab_id)
                    .ok_or(ReduceError::TabNotFound(tab_id))?;
                (tab.workspace_id, tab.profile_id, tab.runtime_state)
            };
            let workspace_active = state
                .workspaces
                .get(&workspace_id)
                .ok_or(ReduceError::WorkspaceNotFound(workspace_id))?
                .active_tab_id
                == Some(tab_id);
            let profile_active_workspace = state
                .profiles
                .get(&profile_id)
                .ok_or(ReduceError::ProfileNotFound(profile_id))?
                .active_workspace_id
                == Some(workspace_id);
            let profile_active = state.active_profile_id == Some(profile_id);
            if workspace_active
                && profile_active_workspace
                && profile_active
                && runtime_state == TabRuntimeState::Active
            {
                return Ok(ops);
            }

            let previous_active = state
                .workspaces
                .get(&workspace_id)
                .ok_or(ReduceError::WorkspaceNotFound(workspace_id))?
                .active_tab_id;

            if let Some(previous_active_tab_id) = previous_active {
                if previous_active_tab_id != tab_id {
                    if let Some(previous_active_tab) = state.tabs.get_mut(&previous_active_tab_id) {
                        previous_active_tab.runtime_state = TabRuntimeState::Warm;
                        ops.push(PatchOp::UpsertTab(previous_active_tab.clone()));
                    }
                }
            }

            {
                let tab = state
                    .tabs
                    .get_mut(&tab_id)
                    .ok_or(ReduceError::TabNotFound(tab_id))?;
                tab.runtime_state = TabRuntimeState::Active;
                ops.push(PatchOp::UpsertTab(tab.clone()));
            }

            {
                let workspace = state
                    .workspaces
                    .get_mut(&workspace_id)
                    .ok_or(ReduceError::WorkspaceNotFound(workspace_id))?;
                workspace.active_tab_id = Some(tab_id);
                ops.push(PatchOp::UpsertWorkspace(workspace.clone()));
            }

            {
                let profile = state
                    .profiles
                    .get_mut(&profile_id)
                    .ok_or(ReduceError::ProfileNotFound(profile_id))?;
                profile.active_workspace_id = Some(workspace_id);
                ops.push(PatchOp::UpsertProfile(profile.clone()));
            }

            state.active_profile_id = Some(profile_id);

            ops.push(PatchOp::SetActiveProfile { profile_id });
            ops.push(PatchOp::SetActiveWorkspace {
                profile_id,
                workspace_id,
            });
            ops.push(PatchOp::SetActiveTab {
                workspace_id,
                tab_id: Some(tab_id),
            });
        }
        Intent::CloseTab { tab_id } => {
            let tab = state
                .tabs
                .remove(&tab_id)
                .ok_or(ReduceError::TabNotFound(tab_id))?;
            let workspace_id = tab.workspace_id;

            let mut active_changed = false;
            let mut new_active_id = None;
            {
                let workspace = state
                    .workspaces
                    .get_mut(&workspace_id)
                    .ok_or(ReduceError::WorkspaceNotFound(workspace_id))?;
                workspace.tab_order.retain(|id| *id != tab_id);

                if workspace.active_tab_id == Some(tab_id) {
                    workspace.active_tab_id = workspace.tab_order.first().copied();
                    new_active_id = workspace.active_tab_id;
                    active_changed = true;
                }

                ops.push(PatchOp::UpsertWorkspace(workspace.clone()));
            }

            if active_changed {
                if let Some(new_active_tab_id) = new_active_id {
                    if let Some(new_active_tab) = state.tabs.get_mut(&new_active_tab_id) {
                        new_active_tab.runtime_state = TabRuntimeState::Active;
                        ops.push(PatchOp::UpsertTab(new_active_tab.clone()));
                    }
                }

                ops.push(PatchOp::SetActiveTab {
                    workspace_id,
                    tab_id: new_active_id,
                });
            }

            ops.push(PatchOp::RemoveTab {
                tab_id,
                workspace_id,
            });
        }
        Intent::MoveTab {
            tab_id,
            workspace_id,
            index,
        } => {
            let (source_workspace_id, source_profile_id) = {
                let tab = state
                    .tabs
                    .get(&tab_id)
                    .ok_or(ReduceError::TabNotFound(tab_id))?;
                (tab.workspace_id, tab.profile_id)
            };
            let target_profile_id = state
                .workspaces
                .get(&workspace_id)
                .ok_or(ReduceError::WorkspaceNotFound(workspace_id))?
                .profile_id;
            if source_profile_id != target_profile_id {
                return Err(ReduceError::CrossProfileMove {
                    tab_id,
                    from_profile: source_profile_id,
                    to_profile: target_profile_id,
                });
            }

            if source_workspace_id == workspace_id {
                let workspace = state
                    .workspaces
                    .get_mut(&workspace_id)
                    .ok_or(ReduceError::WorkspaceNotFound(workspace_id))?;
                workspace.tab_order.retain(|id| *id != tab_id);
                let insert_at = index.min(workspace.tab_order.len());
                workspace.tab_order.insert(insert_at, tab_id);
                ops.push(PatchOp::UpsertWorkspace(workspace.clone()));
                return Ok(ops);
            }

            let mut source_active_changed = false;
            let mut source_new_active = None;
            {
                let source_workspace = state
                    .workspaces
                    .get_mut(&source_workspace_id)
                    .ok_or(ReduceError::WorkspaceNotFound(source_workspace_id))?;
                source_workspace.tab_order.retain(|id| *id != tab_id);
                if source_workspace.active_tab_id == Some(tab_id) {
                    source_workspace.active_tab_id = source_workspace.tab_order.first().copied();
                    source_new_active = source_workspace.active_tab_id;
                    source_active_changed = true;
                }
                ops.push(PatchOp::UpsertWorkspace(source_workspace.clone()));
            }

            {
                let target_workspace = state
                    .workspaces
                    .get_mut(&workspace_id)
                    .ok_or(ReduceError::WorkspaceNotFound(workspace_id))?;
                let insert_at = index.min(target_workspace.tab_order.len());
                target_workspace.tab_order.insert(insert_at, tab_id);
                ops.push(PatchOp::UpsertWorkspace(target_workspace.clone()));
            }

            {
                let tab = state
                    .tabs
                    .get_mut(&tab_id)
                    .ok_or(ReduceError::TabNotFound(tab_id))?;
                tab.workspace_id = workspace_id;
                if source_active_changed {
                    tab.runtime_state = TabRuntimeState::Discarded;
                }
                ops.push(PatchOp::UpsertTab(tab.clone()));
            }

            if source_active_changed {
                if let Some(new_active_id) = source_new_active {
                    if let Some(new_active_tab) = state.tabs.get_mut(&new_active_id) {
                        new_active_tab.runtime_state = TabRuntimeState::Active;
                        ops.push(PatchOp::UpsertTab(new_active_tab.clone()));
                    }
                }
                ops.push(PatchOp::SetActiveTab {
                    workspace_id: source_workspace_id,
                    tab_id: source_new_active,
                });
            }
        }
        Intent::DiscardTab { tab_id } => {
            let (workspace_id, is_active) = {
                let tab = state
                    .tabs
                    .get(&tab_id)
                    .ok_or(ReduceError::TabNotFound(tab_id))?;
                let workspace = state
                    .workspaces
                    .get(&tab.workspace_id)
                    .ok_or(ReduceError::WorkspaceNotFound(tab.workspace_id))?;
                (tab.workspace_id, workspace.active_tab_id == Some(tab_id))
            };
            if is_active {
                return Err(ReduceError::CannotDiscardActiveTab(tab_id));
            }

            let tab = state
                .tabs
                .get_mut(&tab_id)
                .ok_or(ReduceError::TabNotFound(tab_id))?;
            tab.runtime_state = TabRuntimeState::Discarded;
            ops.push(PatchOp::UpsertTab(tab.clone()));
            ops.push(PatchOp::SetActiveTab {
                workspace_id,
                tab_id: state
                    .workspaces
                    .get(&workspace_id)
                    .and_then(|workspace| workspace.active_tab_id),
            });
        }
        Intent::SettingSet { key, value } => {
            state.settings.insert(key.clone(), value.clone());
            ops.push(PatchOp::SettingChanged { key, value });
        }
    }

    Ok(ops)
}
