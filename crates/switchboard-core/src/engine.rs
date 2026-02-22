use std::convert::Infallible;

use crate::intent::Intent;
use crate::patch::{Patch, Snapshot};
use crate::reducer::{apply_intent, ReduceError};
use crate::state::BrowserState;

pub trait Persistence {
    type Error;

    fn commit(&mut self, state: &BrowserState) -> Result<(), Self::Error>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct NoopPersistence;

impl Persistence for NoopPersistence {
    type Error = Infallible;

    fn commit(&mut self, _state: &BrowserState) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[derive(Debug)]
pub enum EngineError<E> {
    Reduce(ReduceError),
    Persist(E),
}

pub struct Engine<P: Persistence> {
    state: BrowserState,
    revision: u64,
    persistence: P,
}

impl<P: Persistence> Engine<P> {
    pub fn new(persistence: P) -> Self {
        Self {
            state: BrowserState::default(),
            revision: 0,
            persistence,
        }
    }

    pub fn with_state(persistence: P, state: BrowserState, revision: u64) -> Self {
        Self {
            state,
            revision,
            persistence,
        }
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn state(&self) -> &BrowserState {
        &self.state
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            state: self.state.clone(),
            revision: self.revision,
        }
    }

    pub fn dispatch(&mut self, intent: Intent) -> Result<Patch, EngineError<P::Error>> {
        let from_revision = self.revision;
        let ops = apply_intent(&mut self.state, intent).map_err(EngineError::Reduce)?;

        // Contract: write to persistence before emitting the resulting patch.
        self.persistence
            .commit(&self.state)
            .map_err(EngineError::Persist)?;

        let to_revision = if ops.is_empty() {
            from_revision
        } else {
            from_revision + 1
        };
        self.revision = to_revision;

        Ok(Patch {
            ops,
            from_revision,
            to_revision,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use crate::ids::TabId;
    use crate::ids::{ProfileId, WorkspaceId};
    use crate::patch::PatchOp;
    use crate::{BrowserState, Intent, NoopPersistence, SettingValue, TabRuntimeState};

    use super::{Engine, EngineError};

    fn seeded_engine() -> (Engine<NoopPersistence>, crate::ids::WorkspaceId) {
        let mut state = BrowserState::default();
        let profile_id = state.add_profile("Default");
        let workspace_id = state
            .add_workspace(profile_id, "Main")
            .expect("profile should exist");
        (Engine::with_state(NoopPersistence, state, 0), workspace_id)
    }

    fn first_tab_id(
        engine: &Engine<NoopPersistence>,
        workspace_id: crate::ids::WorkspaceId,
    ) -> TabId {
        *engine
            .state()
            .workspaces
            .get(&workspace_id)
            .expect("workspace must exist")
            .tab_order
            .first()
            .expect("workspace should have at least one tab")
    }

    fn next_rand(seed: &mut u64) -> u64 {
        *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        *seed
    }

    fn rand_index(seed: &mut u64, len: usize) -> usize {
        (next_rand(seed) as usize) % len
    }

    fn warm_pool_budget(state: &BrowserState) -> usize {
        match state.settings.get("warm_pool_budget") {
            Some(SettingValue::Int(value)) => (*value).clamp(0, 32) as usize,
            _ => 8,
        }
    }

    fn assert_lifecycle_invariants(state: &BrowserState) {
        if let Some(active_profile_id) = state.active_profile_id {
            assert!(
                state.profiles.contains_key(&active_profile_id),
                "active profile must exist"
            );
        }

        for (profile_id, profile) in &state.profiles {
            if let Some(active_workspace_id) = profile.active_workspace_id {
                assert!(
                    profile.workspace_order.contains(&active_workspace_id),
                    "profile active workspace must be in workspace order"
                );
                let workspace = state
                    .workspaces
                    .get(&active_workspace_id)
                    .expect("profile active workspace must exist");
                assert_eq!(workspace.profile_id, *profile_id);
            }
            for workspace_id in &profile.workspace_order {
                let workspace = state
                    .workspaces
                    .get(workspace_id)
                    .expect("workspace in profile order must exist");
                assert_eq!(workspace.profile_id, *profile_id);
            }
        }

        for (workspace_id, workspace) in &state.workspaces {
            if let Some(active_tab_id) = workspace.active_tab_id {
                assert!(
                    workspace.tab_order.contains(&active_tab_id),
                    "workspace active tab must be in workspace tab order"
                );
                let tab = state
                    .tabs
                    .get(&active_tab_id)
                    .expect("workspace active tab must exist");
                assert_eq!(tab.workspace_id, *workspace_id);
            }
            for tab_id in &workspace.tab_order {
                let tab = state.tabs.get(tab_id).expect("workspace tab must exist");
                assert_eq!(tab.workspace_id, *workspace_id);
                assert_eq!(tab.profile_id, workspace.profile_id);
            }
        }

        for (tab_id, tab) in &state.tabs {
            let workspace = state
                .workspaces
                .get(&tab.workspace_id)
                .expect("tab workspace must exist");
            assert_eq!(workspace.profile_id, tab.profile_id);
            assert!(
                workspace.tab_order.contains(tab_id),
                "tab must exist in its workspace order"
            );
            assert!(
                state.profiles.contains_key(&tab.profile_id),
                "tab profile must exist"
            );
        }

        for (profile_id, entries) in &state.warm_lru {
            assert!(
                state.profiles.contains_key(profile_id),
                "warm_lru profile must exist"
            );
            let mut unique = BTreeSet::new();
            for tab_id in entries {
                assert!(
                    unique.insert(*tab_id),
                    "warm_lru must not contain duplicates"
                );
                let tab = state.tabs.get(tab_id).expect("warm_lru tab must exist");
                assert_eq!(tab.profile_id, *profile_id);
            }
        }

        let active_profile_id = state.active_profile_id;
        let active_tab_id = active_profile_id.and_then(|profile_id| {
            state
                .profiles
                .get(&profile_id)
                .and_then(|profile| profile.active_workspace_id)
                .and_then(|workspace_id| {
                    state
                        .workspaces
                        .get(&workspace_id)
                        .and_then(|workspace| workspace.active_tab_id)
                })
        });

        if let (Some(profile_id), Some(tab_id)) = (active_profile_id, active_tab_id) {
            let lru = state
                .warm_lru
                .get(&profile_id)
                .expect("active profile should have warm_lru entry");
            assert!(
                lru.contains(&tab_id),
                "active tab should be tracked in warm_lru"
            );
        }

        let mut active_count = 0usize;
        let mut warm_count_active_profile = 0usize;
        let warm_budget = warm_pool_budget(state);

        for tab in state.tabs.values() {
            match tab.runtime_state {
                TabRuntimeState::Active => {
                    active_count += 1;
                    assert_eq!(
                        Some(tab.profile_id),
                        active_profile_id,
                        "active tab must belong to active profile"
                    );
                    assert_eq!(
                        Some(tab.id),
                        active_tab_id,
                        "only current active tab may be Active"
                    );
                }
                TabRuntimeState::Warm => {
                    assert_eq!(
                        Some(tab.profile_id),
                        active_profile_id,
                        "warm tabs must belong to active profile"
                    );
                    assert_ne!(Some(tab.id), active_tab_id, "active tab must not be Warm");
                    warm_count_active_profile += 1;
                }
                TabRuntimeState::Discarded => {}
                TabRuntimeState::Restoring => {
                    panic!("restoring state is not expected in reducer-only lifecycle tests")
                }
            }
        }

        let expected_active_count = if active_tab_id.is_some() { 1 } else { 0 };
        assert_eq!(
            active_count, expected_active_count,
            "must have exactly one active tab for current active workspace/profile"
        );
        assert!(
            warm_count_active_profile <= warm_budget,
            "warm tab count {} exceeded budget {}",
            warm_count_active_profile,
            warm_budget
        );
    }

    #[test]
    fn new_active_tab_emits_revisioned_patch() {
        let (mut engine, workspace_id) = seeded_engine();

        let patch = engine
            .dispatch(Intent::NewTab {
                workspace_id,
                url: Some("https://example.com".to_owned()),
                make_active: true,
            })
            .expect("dispatch should succeed");

        assert_eq!(patch.from_revision, 0);
        assert_eq!(patch.to_revision, 1);
        assert!(patch.ops.iter().any(|op| matches!(
            op,
            PatchOp::SetActiveTab {
                workspace_id: op_workspace_id,
                tab_id: Some(_),
            } if *op_workspace_id == workspace_id
        )));
    }

    #[test]
    fn close_active_tab_promotes_next_tab() {
        let (mut engine, workspace_id) = seeded_engine();

        engine
            .dispatch(Intent::NewTab {
                workspace_id,
                url: Some("https://one.example".to_owned()),
                make_active: true,
            })
            .expect("first tab should be created");
        engine
            .dispatch(Intent::NewTab {
                workspace_id,
                url: Some("https://two.example".to_owned()),
                make_active: false,
            })
            .expect("second tab should be created");

        let first_id = first_tab_id(&engine, workspace_id);
        let second_id = engine
            .state()
            .workspaces
            .get(&workspace_id)
            .expect("workspace exists")
            .tab_order[1];

        engine
            .dispatch(Intent::CloseTab { tab_id: first_id })
            .expect("close tab should succeed");

        assert_eq!(
            engine
                .state()
                .workspaces
                .get(&workspace_id)
                .expect("workspace exists")
                .active_tab_id,
            Some(second_id)
        );
    }

    #[test]
    fn cannot_discard_active_tab() {
        let (mut engine, workspace_id) = seeded_engine();

        engine
            .dispatch(Intent::NewTab {
                workspace_id,
                url: Some("https://example.com".to_owned()),
                make_active: true,
            })
            .expect("tab should be created");
        let active_tab_id = first_tab_id(&engine, workspace_id);

        let result = engine.dispatch(Intent::DiscardTab {
            tab_id: active_tab_id,
        });

        assert!(matches!(
            result,
            Err(EngineError::Reduce(crate::reducer::ReduceError::CannotDiscardActiveTab(id))) if id == active_tab_id
        ));
    }

    #[test]
    fn activate_already_active_tab_is_noop() {
        let (mut engine, workspace_id) = seeded_engine();
        engine
            .dispatch(Intent::NewTab {
                workspace_id,
                url: Some("https://example.com".to_owned()),
                make_active: true,
            })
            .expect("tab should be created");
        let active_tab_id = first_tab_id(&engine, workspace_id);
        let revision_before = engine.revision();

        let patch = engine
            .dispatch(Intent::ActivateTab {
                tab_id: active_tab_id,
            })
            .expect("activate active tab should succeed");

        assert!(
            patch.ops.is_empty(),
            "activate on already-active tab should no-op"
        );
        assert_eq!(patch.from_revision, revision_before);
        assert_eq!(patch.to_revision, revision_before);
        assert_eq!(engine.revision(), revision_before);
    }

    #[test]
    fn cannot_delete_last_workspace() {
        let (mut engine, workspace_id) = seeded_engine();

        let result = engine.dispatch(Intent::DeleteWorkspace { workspace_id });

        assert!(matches!(
            result,
            Err(EngineError::Reduce(
                crate::reducer::ReduceError::CannotDeleteLastWorkspace(id)
            )) if id == workspace_id
        ));
    }

    #[test]
    fn delete_active_workspace_promotes_remaining_workspace() {
        let (mut engine, first_workspace_id) = seeded_engine();
        let profile_id = engine
            .state()
            .active_profile_id
            .expect("profile should exist");

        engine
            .dispatch(Intent::NewWorkspace {
                profile_id,
                name: "Secondary".to_owned(),
            })
            .expect("workspace should be created");
        let second_workspace_id = engine
            .state()
            .profiles
            .get(&profile_id)
            .expect("profile should exist")
            .workspace_order
            .iter()
            .copied()
            .find(|id| *id != first_workspace_id)
            .expect("second workspace should exist");

        engine
            .dispatch(Intent::SwitchWorkspace {
                workspace_id: second_workspace_id,
            })
            .expect("switch workspace should succeed");
        engine
            .dispatch(Intent::NewTab {
                workspace_id: second_workspace_id,
                url: Some("https://two.example".to_owned()),
                make_active: true,
            })
            .expect("tab should be created");

        let removed_tab_ids = engine
            .state()
            .workspaces
            .get(&second_workspace_id)
            .expect("second workspace should exist")
            .tab_order
            .clone();

        let patch = engine
            .dispatch(Intent::DeleteWorkspace {
                workspace_id: second_workspace_id,
            })
            .expect("delete workspace should succeed");

        assert!(
            engine
                .state()
                .workspaces
                .get(&second_workspace_id)
                .is_none(),
            "deleted workspace must be removed"
        );
        assert_eq!(
            engine
                .state()
                .profiles
                .get(&profile_id)
                .expect("profile should exist")
                .active_workspace_id,
            Some(first_workspace_id)
        );
        for tab_id in &removed_tab_ids {
            assert!(
                engine.state().tabs.get(tab_id).is_none(),
                "tab from deleted workspace must be removed"
            );
        }
        assert!(patch.ops.iter().any(|op| matches!(
            op,
            PatchOp::RemoveWorkspace {
                workspace_id,
                profile_id: op_profile_id,
            } if *workspace_id == second_workspace_id && *op_profile_id == profile_id
        )));
    }

    #[test]
    fn rename_profile_updates_state_and_patch() {
        let (mut engine, _workspace_id) = seeded_engine();
        let profile_id = engine
            .state()
            .active_profile_id
            .expect("profile should exist");

        let patch = engine
            .dispatch(Intent::RenameProfile {
                profile_id,
                name: "Work".to_owned(),
            })
            .expect("rename profile should succeed");

        let profile = engine
            .state()
            .profiles
            .get(&profile_id)
            .expect("profile should exist");
        assert_eq!(profile.name, "Work");
        assert!(patch.ops.iter().any(|op| matches!(
            op,
            PatchOp::UpsertProfile(profile) if profile.id == profile_id && profile.name == "Work"
        )));
    }

    #[test]
    fn warm_pool_budget_is_enforced_with_lru_order() {
        let (mut engine, workspace_id) = seeded_engine();

        engine
            .dispatch(Intent::SettingSet {
                key: "warm_pool_budget".to_owned(),
                value: SettingValue::Int(1),
            })
            .expect("setting warm pool budget should succeed");

        engine
            .dispatch(Intent::NewTab {
                workspace_id,
                url: Some("https://one.example".to_owned()),
                make_active: true,
            })
            .expect("tab one should be created");
        let tab_one_id = first_tab_id(&engine, workspace_id);

        engine
            .dispatch(Intent::NewTab {
                workspace_id,
                url: Some("https://two.example".to_owned()),
                make_active: true,
            })
            .expect("tab two should be created");
        let tab_two_id = engine
            .state()
            .workspaces
            .get(&workspace_id)
            .expect("workspace exists")
            .active_tab_id
            .expect("tab two should be active");

        engine
            .dispatch(Intent::NewTab {
                workspace_id,
                url: Some("https://three.example".to_owned()),
                make_active: true,
            })
            .expect("tab three should be created");
        let tab_three_id = engine
            .state()
            .workspaces
            .get(&workspace_id)
            .expect("workspace exists")
            .active_tab_id
            .expect("tab three should be active");

        let tab_one = engine
            .state()
            .tabs
            .get(&tab_one_id)
            .expect("tab one should exist");
        let tab_two = engine
            .state()
            .tabs
            .get(&tab_two_id)
            .expect("tab two should exist");
        let tab_three = engine
            .state()
            .tabs
            .get(&tab_three_id)
            .expect("tab three should exist");

        assert_eq!(tab_three.runtime_state, TabRuntimeState::Active);
        assert_eq!(tab_two.runtime_state, TabRuntimeState::Warm);
        assert_eq!(tab_one.runtime_state, TabRuntimeState::Discarded);
    }

    #[test]
    fn switching_profiles_discards_inactive_profile_tabs() {
        let (mut engine, first_workspace_id) = seeded_engine();
        let first_profile_id = engine
            .state()
            .active_profile_id
            .expect("first profile should exist");

        engine
            .dispatch(Intent::NewTab {
                workspace_id: first_workspace_id,
                url: Some("https://first.example".to_owned()),
                make_active: true,
            })
            .expect("first profile tab should be created");
        let first_profile_tab_id = engine
            .state()
            .workspaces
            .get(&first_workspace_id)
            .expect("first workspace should exist")
            .active_tab_id
            .expect("first profile tab should be active");

        engine
            .dispatch(Intent::NewProfile {
                name: "Work".to_owned(),
            })
            .expect("second profile should be created");
        let second_profile_id = engine
            .state()
            .active_profile_id
            .expect("second profile should now be active");
        let second_workspace_id = engine
            .state()
            .profiles
            .get(&second_profile_id)
            .and_then(|profile| profile.active_workspace_id)
            .expect("second profile should have an active workspace");

        engine
            .dispatch(Intent::NewTab {
                workspace_id: second_workspace_id,
                url: Some("https://second.example".to_owned()),
                make_active: true,
            })
            .expect("second profile tab should be created");
        let second_profile_tab_id = engine
            .state()
            .workspaces
            .get(&second_workspace_id)
            .expect("second workspace should exist")
            .active_tab_id
            .expect("second profile tab should be active");

        let first_profile_tab = engine
            .state()
            .tabs
            .get(&first_profile_tab_id)
            .expect("first profile tab should still exist");
        let second_profile_tab = engine
            .state()
            .tabs
            .get(&second_profile_tab_id)
            .expect("second profile tab should exist");

        assert_eq!(
            engine.state().active_profile_id,
            Some(second_profile_id),
            "second profile should be active after switching"
        );
        assert_eq!(second_profile_tab.runtime_state, TabRuntimeState::Active);
        assert_eq!(
            first_profile_tab.runtime_state,
            TabRuntimeState::Discarded,
            "inactive profile tabs should be discarded"
        );

        engine
            .dispatch(Intent::SwitchProfile {
                profile_id: first_profile_id,
            })
            .expect("switching back to first profile should succeed");
        let first_profile_tab = engine
            .state()
            .tabs
            .get(&first_profile_tab_id)
            .expect("first profile tab should still exist");
        let second_profile_tab = engine
            .state()
            .tabs
            .get(&second_profile_tab_id)
            .expect("second profile tab should still exist");
        assert_eq!(first_profile_tab.runtime_state, TabRuntimeState::Active);
        assert_eq!(second_profile_tab.runtime_state, TabRuntimeState::Discarded);
    }

    #[test]
    fn lifecycle_policy_stress_under_profile_workspace_tab_churn() {
        let (mut engine, first_workspace_id) = seeded_engine();
        let mut seed = 0x5A17_C0DE_D15C_AFE5u64;
        let first_profile_id = engine
            .state()
            .active_profile_id
            .expect("default profile should exist");

        engine
            .dispatch(Intent::SettingSet {
                key: "warm_pool_budget".to_owned(),
                value: SettingValue::Int(6),
            })
            .expect("warm pool budget should be configured");

        for idx in 0..2 {
            engine
                .dispatch(Intent::NewProfile {
                    name: format!("Profile {}", idx + 2),
                })
                .expect("profile creation should succeed");
        }
        assert_lifecycle_invariants(engine.state());

        let profile_ids: Vec<ProfileId> = engine.state().profiles.keys().copied().collect();
        assert!(profile_ids.len() >= 3);
        assert!(profile_ids.contains(&first_profile_id));

        for profile_id in &profile_ids {
            for workspace_idx in 0..2 {
                engine
                    .dispatch(Intent::NewWorkspace {
                        profile_id: *profile_id,
                        name: format!("W{}-{}", profile_id.0, workspace_idx + 2),
                    })
                    .expect("workspace creation should succeed");
            }
        }
        assert_lifecycle_invariants(engine.state());

        let all_workspace_ids: Vec<WorkspaceId> =
            engine.state().workspaces.keys().copied().collect();
        assert!(all_workspace_ids.contains(&first_workspace_id));

        for workspace_id in &all_workspace_ids {
            engine
                .dispatch(Intent::NewTab {
                    workspace_id: *workspace_id,
                    url: Some(format!("https://seed-{}.example/active", workspace_id.0)),
                    make_active: true,
                })
                .expect("active tab creation should succeed");
            for i in 0..2 {
                engine
                    .dispatch(Intent::NewTab {
                        workspace_id: *workspace_id,
                        url: Some(format!("https://seed-{}.example/{i}", workspace_id.0)),
                        make_active: false,
                    })
                    .expect("background tab creation should succeed");
            }
            assert_lifecycle_invariants(engine.state());
        }

        for step in 0..1500usize {
            let op = rand_index(&mut seed, 5);
            match op {
                0 => {
                    let profile_ids: Vec<ProfileId> =
                        engine.state().profiles.keys().copied().collect();
                    let profile_id = profile_ids[rand_index(&mut seed, profile_ids.len())];
                    engine
                        .dispatch(Intent::SwitchProfile { profile_id })
                        .expect("switch profile should succeed");
                }
                1 => {
                    let workspace_ids: Vec<WorkspaceId> =
                        engine.state().workspaces.keys().copied().collect();
                    let workspace_id = workspace_ids[rand_index(&mut seed, workspace_ids.len())];
                    engine
                        .dispatch(Intent::SwitchWorkspace { workspace_id })
                        .expect("switch workspace should succeed");
                }
                2 => {
                    let tab_ids: Vec<TabId> = engine.state().tabs.keys().copied().collect();
                    if !tab_ids.is_empty() {
                        let tab_id = tab_ids[rand_index(&mut seed, tab_ids.len())];
                        engine
                            .dispatch(Intent::ActivateTab { tab_id })
                            .expect("activate tab should succeed");
                    }
                }
                3 => {
                    let workspace_ids: Vec<WorkspaceId> =
                        engine.state().workspaces.keys().copied().collect();
                    let workspace_id = workspace_ids[rand_index(&mut seed, workspace_ids.len())];
                    let make_active = (next_rand(&mut seed) & 1) == 0;
                    engine
                        .dispatch(Intent::NewTab {
                            workspace_id,
                            url: Some(format!("https://stress.example/{step}")),
                            make_active,
                        })
                        .expect("new tab should succeed");
                }
                _ => {
                    let tab_ids: Vec<TabId> = engine.state().tabs.keys().copied().collect();
                    if !tab_ids.is_empty() {
                        let tab_id = tab_ids[rand_index(&mut seed, tab_ids.len())];
                        engine
                            .dispatch(Intent::CloseTab { tab_id })
                            .expect("close tab should succeed");
                    }
                }
            }
            assert_lifecycle_invariants(engine.state());
        }
    }
}
