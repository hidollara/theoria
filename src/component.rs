//! Typed, directly delivered events for higher-level components.
//!
//! This module supplies the small runtime boundary above [`crate::reactive`].
//! Delivery is direct: routing, gesture recognition, and visual trees can be
//! expressed by components instead of being built into the delivery primitive.
//! Typed event sources provide lifecycle-bound publication without defining
//! how an event should be routed.

mod arena;
mod context;
mod event;
mod runtime;
mod target;

use std::{fmt, marker::PhantomData, rc::Weak};

use arena::ComponentId;
pub use context::{ComponentContext, MountContext};
pub use event::{EventEmitter, EventSource};
use runtime::ComponentRuntimeInner;
pub use runtime::{ComponentRuntime, TurnBudget, TurnOutcome};
pub use target::EventTarget;

trait ComponentResource {
    fn deactivate(&self);
}

/// A stateful event receiver owned by a [`ComponentRuntime`].
///
/// `Event` is normally an enum containing all events accepted by this
/// component. Individual shared event types can implement `Into<Event>`, so a
/// sender does not need to construct the enum wrapper explicitly.
pub trait Component: 'static {
    type Event: 'static;

    fn receive(&mut self, event: Self::Event, context: &mut ComponentContext<'_>);
}

/// A typed, non-owning reference to a mounted component.
///
/// Cloning a handle does not keep its component alive. Handles use a generation
/// number, so a stale handle cannot address a component later mounted into the
/// same arena slot.
pub struct ComponentHandle<C: Component> {
    runtime: Weak<ComponentRuntimeInner>,
    id: ComponentId,
    marker: PhantomData<fn() -> C>,
}

impl<C: Component> ComponentHandle<C> {
    fn new(runtime: Weak<ComponentRuntimeInner>, id: ComponentId) -> Self {
        Self {
            runtime,
            id,
            marker: PhantomData,
        }
    }

    fn runtime(&self) -> Option<std::rc::Rc<ComponentRuntimeInner>> {
        self.runtime.upgrade()
    }

    fn id(&self) -> ComponentId {
        self.id
    }
}

impl<C: Component> Clone for ComponentHandle<C> {
    fn clone(&self) -> Self {
        Self::new(Weak::clone(&self.runtime), self.id)
    }
}

impl<C: Component> fmt::Debug for ComponentHandle<C> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ComponentHandle")
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests;
