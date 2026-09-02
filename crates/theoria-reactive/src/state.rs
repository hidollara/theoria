use std::{
    any::Any,
    cell::{Ref, RefCell},
    fmt,
    rc::Rc,
};

use crate::{
    error::ReactiveError,
    node::{ConsumerSet, Provider, ProviderAccess},
    operation::{OperationGate, OperationGuard},
    read::Read,
    value::Value,
};

/// An owning mutable State without persistent snapshots.
///
/// [`State::read`] borrows the current value without tracking a dependency.
/// Tracked evaluation instead reads this State through [`crate::ReadContext`].
/// Neither form requires cloning the stored value.
pub struct State<T> {
    value: Value<T>,
}

impl<T> State<T> {
    pub(crate) fn new(gate: &Rc<OperationGate>, value: T) -> Self
    where
        T: 'static,
    {
        let provider: Rc<dyn Provider> = Rc::new(StateNode::new(Rc::clone(gate), value));
        Self {
            value: Value::new(provider),
        }
    }

    #[cfg(test)]
    pub(crate) fn provider(&self) -> Rc<dyn Provider> {
        self.value.provider()
    }

    fn stored(&self) -> &StateNode<T>
    where
        T: 'static,
    {
        let provider: &dyn Any = self.value.provider_ref();
        provider
            .downcast_ref::<StateNode<T>>()
            .unwrap_or_else(|| unreachable!("State handle must preserve its value type"))
    }

    fn change_in<R>(
        &self,
        operation: &OperationGuard<'_>,
        modification: impl FnOnce(&StateNode<T>) -> R,
    ) -> R
    where
        T: 'static,
    {
        assert!(
            operation.is_for(self.value.provider_ref().gate()),
            "reactive State belongs to another Reactive"
        );
        self.value.provider_ref().invalidate_consumers();
        modification(self.stored())
    }

    pub(crate) fn set_in(&self, operation: &OperationGuard<'_>, value: T)
    where
        T: 'static,
    {
        drop(self.replace_in(operation, value));
    }

    pub(crate) fn modify_in(&self, operation: &OperationGuard<'_>, modify: impl FnOnce(&mut T))
    where
        T: 'static,
    {
        self.change_in(operation, |stored| stored.modify(modify));
    }

    pub(crate) fn update_in(&self, operation: &OperationGuard<'_>, update: impl FnOnce(&T) -> T)
    where
        T: 'static,
    {
        assert!(
            operation.is_for(self.value.provider_ref().gate()),
            "reactive State belongs to another Reactive"
        );
        let updated = {
            let current = self.stored().value.borrow();
            update(&current)
        };
        self.value.provider_ref().invalidate_consumers();
        drop(self.stored().replace(updated));
    }

    pub(crate) fn replace_in(&self, operation: &OperationGuard<'_>, value: T) -> T
    where
        T: 'static,
    {
        self.change_in(operation, |stored| stored.replace(value))
    }

    /// Returns a read-only handle without State's mutation capabilities.
    pub fn value(&self) -> Value<T> {
        self.value.clone()
    }

    /// Borrows the current value without registering a reactive dependency.
    ///
    /// This State's Reactive remains exclusively occupied until the returned
    /// view is dropped, preventing mutation or evaluation from overlapping the
    /// borrowed value.
    pub fn read(&self) -> Result<Read<'_, T>, ReactiveError>
    where
        T: 'static,
    {
        let operation = self.value.provider_ref().gate().acquire()?;
        Ok(Read::guarded(self.stored().value.borrow(), operation))
    }

    /// Replaces this State as one mutation batch.
    pub fn set(&self, value: T) -> Result<(), ReactiveError>
    where
        T: 'static,
    {
        let operation = self.value.provider_ref().gate().acquire()?;
        self.set_in(&operation, value);
        Ok(())
    }

    /// Mutates this State in place as one mutation batch.
    pub fn modify(&self, modify: impl FnOnce(&mut T)) -> Result<(), ReactiveError>
    where
        T: 'static,
    {
        let operation = self.value.provider_ref().gate().acquire()?;
        self.modify_in(&operation, modify);
        Ok(())
    }

    /// Replaces the current value with an update derived from it.
    pub fn update(&self, update: impl FnOnce(&T) -> T) -> Result<(), ReactiveError>
    where
        T: 'static,
    {
        let operation = self.value.provider_ref().gate().acquire()?;
        self.update_in(&operation, update);
        Ok(())
    }

    /// Replaces this State and returns ownership of its previous value.
    pub fn replace(&self, value: T) -> Result<T, ReactiveError>
    where
        T: 'static,
    {
        let operation = self.value.provider_ref().gate().acquire()?;
        Ok(self.replace_in(&operation, value))
    }
}

impl<T> Clone for State<T> {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
        }
    }
}

impl<T> AsRef<Value<T>> for State<T> {
    fn as_ref(&self) -> &Value<T> {
        &self.value
    }
}

impl<T> fmt::Debug for State<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("State").finish_non_exhaustive()
    }
}

struct StateNode<T> {
    gate: Rc<OperationGate>,
    consumers: ConsumerSet,
    value: RefCell<T>,
}

impl<T> StateNode<T> {
    fn new(gate: Rc<OperationGate>, value: T) -> Self {
        Self {
            gate,
            consumers: ConsumerSet::new(),
            value: RefCell::new(value),
        }
    }

    fn replace(&self, value: T) -> T {
        self.value.replace(value)
    }

    fn modify(&self, modify: impl FnOnce(&mut T)) {
        modify(&mut self.value.borrow_mut());
    }
}

impl<T: 'static> Provider for StateNode<T> {
    fn gate(&self) -> &Rc<OperationGate> {
        &self.gate
    }

    fn consumers(&self) -> &ConsumerSet {
        &self.consumers
    }

    fn access(&self) -> ProviderAccess<'_> {
        ProviderAccess::Borrowed(Ref::map(self.value.borrow(), |value| value as &dyn Any))
    }
}
