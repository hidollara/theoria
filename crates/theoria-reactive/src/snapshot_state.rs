use std::{any::Any, cell::RefCell, fmt, rc::Rc};

use crate::{
    error::ReactiveError,
    evaluation::Evaluation,
    node::{ConsumerSet, Provider, ProviderAccess},
    operation::{OperationGate, OperationGuard},
    read::Snapshot,
    value::Value,
};

/// An owning mutable state whose immutable values may be retained by Snapshot.
pub struct SnapshotState<T> {
    value: Value<T>,
}

impl<T> SnapshotState<T> {
    pub(crate) fn new(gate: &Rc<OperationGate>, value: T) -> Self
    where
        T: 'static,
    {
        let provider: Rc<dyn Provider> = Rc::new(SnapshotStateNode::new(Rc::clone(gate), value));
        Self {
            value: Value::new(provider),
        }
    }

    pub(crate) fn provider(&self) -> Rc<dyn Provider> {
        self.value.provider()
    }

    fn stored(&self) -> &SnapshotStateNode<T>
    where
        T: 'static,
    {
        let provider: &dyn Any = self.value.provider_ref();
        provider
            .downcast_ref::<SnapshotStateNode<T>>()
            .unwrap_or_else(|| unreachable!("SnapshotState handle must preserve its value type"))
    }

    pub(crate) fn set_in(&self, operation: &OperationGuard<'_>, value: T)
    where
        T: 'static,
    {
        assert!(
            operation.is_for(self.value.provider_ref().gate()),
            "reactive SnapshotState belongs to another Reactive"
        );
        self.value.provider_ref().invalidate_consumers();
        self.stored().set(value);
    }

    pub(crate) fn update_in(&self, operation: &OperationGuard<'_>, update: impl FnOnce(&T) -> T)
    where
        T: 'static,
    {
        assert!(
            operation.is_for(self.value.provider_ref().gate()),
            "reactive SnapshotState belongs to another Reactive"
        );
        let updated = {
            let current = self.stored().value.borrow();
            update(current.as_ref())
        };
        self.value.provider_ref().invalidate_consumers();
        self.stored().set(updated);
    }

    /// Returns a read-only handle without SnapshotState-specific capabilities.
    pub fn value(&self) -> Value<T> {
        self.value.clone()
    }

    /// Returns an immutable snapshot that remains valid after later writes.
    pub fn snapshot(&self) -> Result<Snapshot<T>, ReactiveError>
    where
        T: 'static,
    {
        let evaluation = Evaluation::new(self.value.provider_ref().gate())?;
        Ok(Snapshot::new(evaluation.resolve(self.provider())))
    }

    /// Replaces this SnapshotState as one mutation batch.
    pub fn set(&self, value: T) -> Result<(), ReactiveError>
    where
        T: 'static,
    {
        let operation = self.value.provider_ref().gate().acquire()?;
        self.set_in(&operation, value);
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
}

impl<T> Clone for SnapshotState<T> {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
        }
    }
}

impl<T> AsRef<Value<T>> for SnapshotState<T> {
    fn as_ref(&self) -> &Value<T> {
        &self.value
    }
}

impl<T> fmt::Debug for SnapshotState<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SnapshotState")
            .finish_non_exhaustive()
    }
}

struct SnapshotStateNode<T> {
    gate: Rc<OperationGate>,
    consumers: ConsumerSet,
    value: RefCell<Rc<T>>,
}

impl<T> SnapshotStateNode<T> {
    fn new(gate: Rc<OperationGate>, value: T) -> Self {
        Self {
            gate,
            consumers: ConsumerSet::new(),
            value: RefCell::new(Rc::new(value)),
        }
    }

    fn set(&self, value: T) {
        self.value.replace(Rc::new(value));
    }
}

impl<T: 'static> Provider for SnapshotStateNode<T> {
    fn gate(&self) -> &Rc<OperationGate> {
        &self.gate
    }

    fn consumers(&self) -> &ConsumerSet {
        &self.consumers
    }

    fn access(&self) -> ProviderAccess<'_> {
        ProviderAccess::Shared(self.value.borrow().clone())
    }
}
