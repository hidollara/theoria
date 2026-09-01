use std::{any::Any, fmt, marker::PhantomData, rc::Weak};

use super::{Component, ComponentHandle, arena::ComponentId, runtime::ComponentRuntimeInner};

/// A type-erased direct-delivery destination for one public event type.
///
/// Targets let a router store heterogeneous components without erasing the
/// event vocabulary it delivers. Like [`ComponentHandle`], a target is
/// non-owning and becomes stale when its component is unmounted.
pub struct EventTarget<E: 'static> {
    runtime: Weak<ComponentRuntimeInner>,
    id: ComponentId,
    convert: fn(E) -> Box<dyn Any>,
    marker: PhantomData<fn(E)>,
}

impl<E: 'static> EventTarget<E> {
    fn new<C>(handle: &ComponentHandle<C>) -> Self
    where
        C: Component,
        E: Into<C::Event>,
    {
        Self {
            runtime: Weak::clone(&handle.runtime),
            id: handle.id,
            convert: convert::<C, E>,
            marker: PhantomData,
        }
    }

    pub(super) fn runtime(&self) -> Option<std::rc::Rc<ComponentRuntimeInner>> {
        self.runtime.upgrade()
    }

    pub(super) fn id(&self) -> ComponentId {
        self.id
    }

    pub(super) fn erase(&self, event: E) -> Box<dyn Any> {
        (self.convert)(event)
    }

    pub(crate) fn is_active(&self) -> bool {
        self.runtime()
            .is_some_and(|runtime| runtime.accepts_events(self.id))
    }
}

impl<E: 'static> Clone for EventTarget<E> {
    fn clone(&self) -> Self {
        Self {
            runtime: Weak::clone(&self.runtime),
            id: self.id,
            convert: self.convert,
            marker: PhantomData,
        }
    }
}

impl<E: 'static> fmt::Debug for EventTarget<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EventTarget")
            .finish_non_exhaustive()
    }
}

impl<C: Component> ComponentHandle<C> {
    /// Erases this component's concrete type while preserving an accepted
    /// input event type.
    pub fn event_target<E>(&self) -> EventTarget<E>
    where
        E: Into<C::Event> + 'static,
    {
        EventTarget::new(self)
    }
}

fn convert<C, E>(event: E) -> Box<dyn Any>
where
    C: Component,
    E: Into<C::Event> + 'static,
{
    Box::new(event.into())
}
