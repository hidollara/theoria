use std::{fmt, rc::Rc};

use crate::{
    computed::Computed, effect::Effect, error::ReactiveError, operation::OperationGate,
    read::ReadContext, snapshot_state::SnapshotState, state::State, write::WriteContext,
};

/// Creates and coordinates related reactive nodes.
///
/// Reactive does not own the nodes it creates. State, Computed, and Effect
/// handles own their nodes, while dependency edges retain the providers needed
/// for evaluation. Clone this factory when several construction sites should
/// create mutually dependent nodes.
#[derive(Clone)]
pub struct Reactive {
    gate: Rc<OperationGate>,
}

impl Reactive {
    /// Creates an independent Reactive.
    pub fn new() -> Self {
        Self {
            gate: OperationGate::new(),
        }
    }

    /// Runs several State writes as one isolated mutation batch.
    ///
    /// Other top-level operations through this Reactive cannot overlap the
    /// callback. Writes completed before a callback panic are not rolled back.
    pub fn mutate<R>(
        &self,
        mutation: impl FnOnce(WriteContext<'_>) -> R,
    ) -> Result<R, ReactiveError> {
        let context = WriteContext::new(&self.gate)?;
        Ok(mutation(context))
    }

    /// Creates an owning mutable State coordinated by this Reactive.
    pub fn create_state<T: 'static>(&self, value: T) -> State<T> {
        State::new(&self.gate, value)
    }

    /// Creates SnapshotState whose immutable values may outlive later writes.
    pub fn create_snapshot_state<T: 'static>(&self, value: T) -> SnapshotState<T> {
        SnapshotState::new(&self.gate, value)
    }

    /// Creates a lazily evaluated Computed result coordinated by this Reactive.
    ///
    /// Reads performed through the callback's [`ReadContext`] become tracked
    /// dependencies after each successful evaluation.
    pub fn create_computed<T: 'static>(
        &self,
        builder: impl Fn(&mut ReadContext<'_>) -> T + 'static,
    ) -> Computed<T> {
        Computed::new(&self.gate, builder)
    }

    /// Creates an explicitly executable Effect coordinated by this Reactive.
    pub fn create_effect(&self, effect: impl FnMut(&mut ReadContext<'_>) + 'static) -> Effect {
        Effect::new(&self.gate, effect)
    }
}

impl Default for Reactive {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for Reactive {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("Reactive").finish_non_exhaustive()
    }
}
