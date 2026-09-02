use std::{fmt, marker::PhantomData, rc::Rc};

use crate::node::Provider;

/// An owning read-only handle to a reactive value.
///
/// Value does not expose mutation or persistent snapshots. State,
/// SnapshotState and Computed retain those capabilities on their own types.
pub struct Value<T> {
    provider: Rc<dyn Provider>,
    marker: PhantomData<fn() -> T>,
}

impl<T> Value<T> {
    pub(crate) fn new(provider: Rc<dyn Provider>) -> Self {
        Self {
            provider,
            marker: PhantomData,
        }
    }

    pub(crate) fn provider(&self) -> Rc<dyn Provider> {
        Rc::clone(&self.provider)
    }

    pub(crate) fn provider_ref(&self) -> &dyn Provider {
        self.provider.as_ref()
    }
}

impl<T> Clone for Value<T> {
    fn clone(&self) -> Self {
        Self::new(Rc::clone(&self.provider))
    }
}

impl<T> AsRef<Value<T>> for Value<T> {
    fn as_ref(&self) -> &Value<T> {
        self
    }
}

impl<T> fmt::Debug for Value<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("Value").finish_non_exhaustive()
    }
}
