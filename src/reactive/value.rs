use std::{
    fmt,
    marker::PhantomData,
    ops::Deref,
    rc::{Rc, Weak},
};

use super::runtime::{NodeId, RuntimeInner};

/// A read-only value with a private physical representation.
pub struct Value<T> {
    pub(super) inner: ValueInner<T>,
}

pub(super) enum ValueInner<T> {
    Constant(Rc<T>),
    Reactive(DynamicHandle<T>),
}

impl<T> Value<T> {
    pub fn constant(value: T) -> Self {
        Self {
            inner: ValueInner::Constant(Rc::new(value)),
        }
    }

    pub(super) fn reactive(handle: DynamicHandle<T>) -> Self {
        Self {
            inner: ValueInner::Reactive(handle),
        }
    }
}

impl<T> From<T> for Value<T> {
    fn from(value: T) -> Self {
        Self::constant(value)
    }
}

impl<T> Clone for Value<T> {
    fn clone(&self) -> Self {
        let inner = match &self.inner {
            ValueInner::Constant(value) => ValueInner::Constant(Rc::clone(value)),
            ValueInner::Reactive(handle) => ValueInner::Reactive(handle.clone()),
        };
        Self { inner }
    }
}

impl<T> fmt::Debug for Value<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("Value").finish_non_exhaustive()
    }
}

/// A State source whose read capability is exposed as [`Value<T>`].
///
/// State does not expose mutation directly. A Runtime-created
/// [`super::EventContext`] is required so writes cannot occur during Computed
/// or Effect evaluation.
pub struct State<T> {
    pub(super) handle: DynamicHandle<T>,
}

impl<T: 'static> State<T> {
    pub(super) fn from_handle(handle: DynamicHandle<T>) -> Self {
        Self { handle }
    }

    pub fn value(&self) -> Value<T> {
        Value::reactive(self.handle.clone())
    }
}

impl<T> Clone for State<T> {
    fn clone(&self) -> Self {
        Self {
            handle: self.handle.clone(),
        }
    }
}

impl<T> fmt::Debug for State<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("State").finish_non_exhaustive()
    }
}

/// The result of an incremental computed evaluation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ComputeResult<T> {
    /// Retains the previous result and its published revision.
    Keep,
    /// Publishes a replacement and advances the computed revision.
    Replace(T),
}

/// An immutable resolved value that remains valid after its source changes.
pub struct Snapshot<T> {
    value: Rc<T>,
}

impl<T> Snapshot<T> {
    pub(super) fn new(value: Rc<T>) -> Self {
        Self { value }
    }

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

pub(super) struct DynamicHandle<T> {
    runtime: Weak<RuntimeInner>,
    pub(super) id: NodeId,
    marker: PhantomData<fn() -> T>,
}

impl<T> DynamicHandle<T> {
    pub(super) fn new(runtime: Weak<RuntimeInner>, id: NodeId) -> Self {
        Self {
            runtime,
            id,
            marker: PhantomData,
        }
    }

    pub(super) fn runtime(&self) -> Rc<RuntimeInner> {
        self.runtime
            .upgrade()
            .expect("reactive value used after its Runtime was dropped")
    }
}

impl<T> Clone for DynamicHandle<T> {
    fn clone(&self) -> Self {
        Self::new(Weak::clone(&self.runtime), self.id)
    }
}
