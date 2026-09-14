use std::{
    any::Any,
    fmt,
    rc::{Rc, Weak},
};

use crate::{
    dispatch::{Dispatch, DispatchOutcome},
    sink::EventSink,
};

type ReceiveFn<E> = dyn Fn(&dyn Any, E);

pub(crate) trait EventEndpoint: 'static {
    type Event;

    fn receive(&self, event: Self::Event);
}

/// A cloneable, non-owning reference to an event endpoint.
///
/// An [`EventTarget`] does not keep its endpoint alive. [`EventSink::send`]
/// creates a [`TargetDispatch`], which checks the endpoint when it runs.
pub struct EventTarget<E> {
    endpoint: Weak<dyn Any>,
    receive: Rc<ReceiveFn<E>>,
}

impl<E> EventTarget<E> {
    pub(crate) fn from_endpoint<T>(endpoint: Weak<T>) -> Self
    where
        T: EventEndpoint<Event = E>,
    {
        Self {
            endpoint,
            receive: Rc::new(move |endpoint, event| {
                let endpoint = endpoint
                    .downcast_ref::<T>()
                    .expect("an EventTarget endpoint must retain its original type");
                endpoint.receive(event);
            }),
        }
    }

    /// Delivers one event immediately, returning it if the endpoint is gone.
    pub fn send_inline(&self, event: E) -> Result<(), E> {
        let Some(endpoint) = self.endpoint.upgrade() else {
            return Err(event);
        };
        (self.receive)(endpoint.as_ref(), event);
        Ok(())
    }

    pub(crate) fn deliver(&self, event: E) -> DispatchOutcome {
        let Some(endpoint) = self.endpoint.upgrade() else {
            return DispatchOutcome::SinkClosed;
        };
        (self.receive)(endpoint.as_ref(), event);
        DispatchOutcome::Completed
    }
}

impl<E> EventSink<E> for EventTarget<E> {
    type Dispatch = TargetDispatch<E>;

    fn send(self, event: E) -> Self::Dispatch {
        TargetDispatch {
            target: self,
            event,
        }
    }
}

impl<E> EventSink<E> for &EventTarget<E> {
    type Dispatch = TargetDispatch<E>;

    fn send(self, event: E) -> Self::Dispatch {
        EventSink::send(EventTarget::clone(self), event)
    }
}

/// One concrete invocation of an [`EventTarget`].
#[must_use = "a TargetDispatch must be run, submitted, or intentionally discarded"]
pub struct TargetDispatch<E> {
    target: EventTarget<E>,
    event: E,
}

impl<E> Dispatch for TargetDispatch<E> {
    fn run(self) -> DispatchOutcome {
        self.target.deliver(self.event)
    }
}

impl<E> fmt::Debug for TargetDispatch<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TargetDispatch")
            .finish_non_exhaustive()
    }
}

impl<E> Clone for EventTarget<E> {
    fn clone(&self) -> Self {
        Self {
            endpoint: Weak::clone(&self.endpoint),
            receive: Rc::clone(&self.receive),
        }
    }
}

impl<E> From<&EventTarget<E>> for EventTarget<E> {
    fn from(target: &EventTarget<E>) -> Self {
        target.clone()
    }
}

impl<E> fmt::Debug for EventTarget<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EventTarget")
            .finish_non_exhaustive()
    }
}
