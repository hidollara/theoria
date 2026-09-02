use std::{
    any::Any,
    cell::{Ref, RefCell},
    fmt,
    ops::Deref,
    rc::Rc,
};

use crate::{
    node::{EvaluationReader, Provider, ProviderAccess},
    operation::OperationGuard,
    value::Value,
};

/// Tracked read capabilities for one Computed evaluation or Effect execution.
///
/// Every value read through this context must belong to the same [`crate::Reactive`]
/// as the active evaluation. Reading an unrelated value is a contract violation
/// and panics.
pub struct ReadContext<'evaluation> {
    reader: RefCell<&'evaluation mut dyn EvaluationReader>,
}

impl<'evaluation> ReadContext<'evaluation> {
    pub(crate) fn new(reader: &'evaluation mut dyn EvaluationReader) -> Self {
        Self {
            reader: RefCell::new(reader),
        }
    }

    /// Reads one reactive value handle and records it as a dependency.
    ///
    /// The returned view cannot outlive this evaluation. State is borrowed in
    /// place; Computed and SnapshotState values are held through their shared
    /// immutable result.
    pub fn read<'read, T: 'static, V>(&'read self, value: &'read V) -> Read<'read, T>
    where
        V: AsRef<Value<T>> + ?Sized,
    {
        let value = value.as_ref();

        match value.provider_ref().access() {
            ProviderAccess::Borrowed(stored) => {
                self.track(value.provider());
                let stored = Ref::map(stored, |stored| {
                    stored
                        .downcast_ref::<T>()
                        .unwrap_or_else(|| unreachable!("Value must preserve its value type"))
                });
                Read::borrowed(stored)
            }
            ProviderAccess::Shared(stored) => {
                self.track(value.provider());
                let stored = stored
                    .downcast::<T>()
                    .unwrap_or_else(|_| panic!("reactive value has an incompatible type"));
                Read::shared(stored)
            }
            ProviderAccess::Computed(_) => {
                let stored = self
                    .resolve(value.provider())
                    .downcast::<T>()
                    .unwrap_or_else(|_| panic!("reactive value has an incompatible type"));
                Read::shared(stored)
            }
        }
    }

    fn track(&self, provider: Rc<dyn Provider>) {
        self.reader.borrow_mut().track(provider);
    }

    fn resolve(&self, provider: Rc<dyn Provider>) -> Rc<dyn Any> {
        self.reader.borrow_mut().resolve_erased(provider)
    }
}

impl fmt::Debug for ReadContext<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReadContext")
            .finish_non_exhaustive()
    }
}

/// A temporary view of a reactive value.
///
/// Reads obtained through [`ReadContext`] participate in dependency tracking.
/// A direct [`crate::State::read`] instead holds exclusive top-level access to
/// its Reactive until this view is dropped.
pub struct Read<'read, T> {
    storage: ReadStorage<'read, T>,
}

impl<'read, T> Read<'read, T> {
    fn borrowed(value: Ref<'read, T>) -> Self {
        Self {
            storage: ReadStorage::Borrowed(value),
        }
    }

    pub(crate) fn guarded(value: Ref<'read, T>, operation: OperationGuard<'read>) -> Self {
        Self {
            storage: ReadStorage::Guarded {
                value,
                _operation: operation,
            },
        }
    }

    fn shared(value: Rc<T>) -> Self {
        Self {
            storage: ReadStorage::Shared(value),
        }
    }

    /// Clones the observed value out of this temporary view.
    pub fn cloned(&self) -> T
    where
        T: Clone,
    {
        self.deref().clone()
    }
}

impl<T> Deref for Read<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match &self.storage {
            ReadStorage::Borrowed(value) => value,
            ReadStorage::Guarded { value, .. } => value,
            ReadStorage::Shared(value) => value,
        }
    }
}

impl<T: fmt::Debug> fmt::Debug for Read<'_, T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.deref().fmt(formatter)
    }
}

enum ReadStorage<'read, T> {
    Borrowed(Ref<'read, T>),
    Guarded {
        value: Ref<'read, T>,
        _operation: OperationGuard<'read>,
    },
    Shared(Rc<T>),
}

/// An immutable resolved value that remains valid after its source changes.
pub struct Snapshot<T> {
    value: Rc<T>,
}

impl<T> Snapshot<T> {
    pub(crate) fn new(value: Rc<T>) -> Self {
        Self { value }
    }

    /// Clones the resolved value out of this Snapshot.
    pub fn cloned(&self) -> T
    where
        T: Clone,
    {
        self.deref().clone()
    }
}

impl<T> Clone for Snapshot<T> {
    fn clone(&self) -> Self {
        Self::new(Rc::clone(&self.value))
    }
}

impl<T> Deref for Snapshot<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T: fmt::Debug> fmt::Debug for Snapshot<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.deref().fmt(formatter)
    }
}
