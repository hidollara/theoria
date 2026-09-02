use std::{fmt, rc::Rc};

use crate::{
    error::ReactiveError,
    operation::{OperationGate, OperationGuard},
    snapshot_state::SnapshotState,
    state::State,
};

/// A borrowed destination accepted by [`WriteContext::set`] and
/// [`WriteContext::update`].
///
/// Both [`State`] and [`SnapshotState`] convert into this adapter without an
/// allocation. Its representation is private so replacing a value remains
/// coordinated by the originating [`crate::Reactive`].
pub struct StateSetter<'state, T> {
    inner: StateSetterInner<'state, T>,
}

enum StateSetterInner<'state, T> {
    State(&'state State<T>),
    SnapshotState(&'state SnapshotState<T>),
}

impl<T: 'static> StateSetter<'_, T> {
    fn set_in(self, operation: &OperationGuard<'_>, value: T) {
        match self.inner {
            StateSetterInner::State(state) => state.set_in(operation, value),
            StateSetterInner::SnapshotState(state) => state.set_in(operation, value),
        }
    }

    fn update_in(self, operation: &OperationGuard<'_>, update: impl FnOnce(&T) -> T) {
        match self.inner {
            StateSetterInner::State(state) => state.update_in(operation, update),
            StateSetterInner::SnapshotState(state) => state.update_in(operation, update),
        }
    }
}

impl<'state, T> From<&'state State<T>> for StateSetter<'state, T> {
    fn from(state: &'state State<T>) -> Self {
        Self {
            inner: StateSetterInner::State(state),
        }
    }
}

impl<'state, T> From<&'state SnapshotState<T>> for StateSetter<'state, T> {
    fn from(state: &'state SnapshotState<T>) -> Self {
        Self {
            inner: StateSetterInner::SnapshotState(state),
        }
    }
}

/// State-writing capabilities for one mutation batch.
///
/// Every State passed to this context must belong to the same
/// [`crate::Reactive`] as the mutation batch. Passing an unrelated State is a
/// contract violation and panics.
pub struct WriteContext<'mutation> {
    operation: OperationGuard<'mutation>,
}

impl<'mutation> WriteContext<'mutation> {
    pub(crate) fn new(gate: &'mutation Rc<OperationGate>) -> Result<Self, ReactiveError> {
        Ok(Self {
            operation: gate.acquire()?,
        })
    }

    /// Replaces State or SnapshotState during this mutation batch.
    pub fn set<'state, T: 'static>(&self, state: impl Into<StateSetter<'state, T>>, value: T) {
        state.into().set_in(&self.operation, value);
    }

    /// Mutates State in place during this mutation batch.
    pub fn modify<T: 'static>(&self, state: &State<T>, modify: impl FnOnce(&mut T)) {
        state.modify_in(&self.operation, modify);
    }

    /// Replaces State or SnapshotState with an update derived from its current value.
    pub fn update<'state, T: 'static>(
        &self,
        state: impl Into<StateSetter<'state, T>>,
        update: impl FnOnce(&T) -> T,
    ) {
        state.into().update_in(&self.operation, update);
    }

    /// Replaces State and returns ownership of its previous value.
    pub fn replace<T: 'static>(&self, state: &State<T>, value: T) -> T {
        state.replace_in(&self.operation, value)
    }
}

impl fmt::Debug for WriteContext<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WriteContext")
            .finish_non_exhaustive()
    }
}
