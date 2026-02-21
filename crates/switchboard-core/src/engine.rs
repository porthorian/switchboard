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
    use crate::ids::TabId;
    use crate::patch::PatchOp;
    use crate::{BrowserState, Intent, NoopPersistence};

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

        assert!(patch.ops.is_empty(), "activate on already-active tab should no-op");
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
            engine.state().workspaces.get(&second_workspace_id).is_none(),
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
}
