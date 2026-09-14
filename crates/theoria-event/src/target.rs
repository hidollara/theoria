use std::{
    any::Any,
    fmt,
    rc::{Rc, Weak},
};

use crate::{
    dispatch::{DispatchOutcome, EventDispatch},
    sink::EventSink,
};

type ReceiveFn<E> = dyn Fn(&dyn Any, E);

pub(crate) trait EventEndpoint: 'static {
    type Event;

    fn receive(&self, event: Self::Event);
}

/// A cloneable, non-owning reference to an event endpoint.
///
/// An [`EventTarget`] does not keep its endpoint alive. [`Self::send`] creates
/// an [`EventDispatch`], which checks the endpoint when it runs.
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

    /// Builds one invocation carrying `event` to this target.
    pub fn send<'event>(&self, event: E) -> EventDispatch<'event>
    where
        E: 'event,
    {
        let target = self.clone();
        EventDispatch::new(move || target.deliver(event))
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
            return DispatchOutcome::SinkUnavailable;
        };
        (self.receive)(endpoint.as_ref(), event);
        DispatchOutcome::Delivered
    }
}

impl<E: 'static> EventSink<E> for EventTarget<E> {
    type Dispatch = EventDispatch<'static>;

    fn send(&self, event: E) -> Self::Dispatch {
        EventTarget::send(self, event)
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
