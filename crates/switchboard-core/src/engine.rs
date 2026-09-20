use std::convert::Infallible;

use crate::intent::Intent;
use crate::patch::{Patch, Snapshot};
use crate::reducer::{apply_intent, ReduceError};
use crate::state::BrowserState;

pub trait Persistence {
    type Error;

    /// Atomically persists the candidate domain state and its committed revision.
    fn commit(&mut self, state: &BrowserState, revision: u64) -> Result<(), Self::Error>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct NoopPersistence;

impl Persistence for NoopPersistence {
    type Error = Infallible;

    fn commit(&mut self, _state: &BrowserState, _revision: u64) -> Result<(), Self::Error> {
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
        Self::with_state(persistence, BrowserState::default(), 0)
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
        let mut candidate = self.state.clone();
        let ops = apply_intent(&mut candidate, intent).map_err(EngineError::Reduce)?;
        if ops.is_empty() {
            return Ok(Patch {
                ops,
                from_revision,
                to_revision: from_revision,
            });
        }

        let to_revision = from_revision.saturating_add(1);
        self.persistence
            .commit(&candidate, to_revision)
            .map_err(EngineError::Persist)?;
        self.state = candidate;
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
    use super::*;
    use crate::{BrowserState, Intent};

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct Fail;

    struct FailingPersistence;

    impl Persistence for FailingPersistence {
        type Error = Fail;

        fn commit(&mut self, _state: &BrowserState, _revision: u64) -> Result<(), Self::Error> {
            Err(Fail)
        }
    }

    fn seeded<P: Persistence>(persistence: P) -> (Engine<P>, crate::WorkspaceId) {
        let mut state = BrowserState::default();
        let profile = state.add_profile("Default");
        let workspace = state.add_workspace(profile, "Main").unwrap();
        (Engine::with_state(persistence, state, 7), workspace)
    }

    #[test]
    fn failed_commit_changes_neither_state_nor_revision() {
        let (mut engine, workspace) = seeded(FailingPersistence);
        let before = engine.state().clone();
        let result = engine.dispatch(Intent::NewTab {
            workspace_id: workspace,
            url: Some("https://example.com".into()),
            make_active: true,
        });
        assert!(matches!(result, Err(EngineError::Persist(Fail))));
        assert_eq!(engine.state(), &before);
        assert_eq!(engine.revision(), 7);
    }

    #[test]
    fn successful_commit_advances_exactly_once() {
        let (mut engine, workspace) = seeded(NoopPersistence);
        let patch = engine
            .dispatch(Intent::NewTab {
                workspace_id: workspace,
                url: Some("https://example.com".into()),
                make_active: true,
            })
            .unwrap();
        assert_eq!((patch.from_revision, patch.to_revision), (7, 8));
        assert_eq!(engine.revision(), 8);
    }
}
