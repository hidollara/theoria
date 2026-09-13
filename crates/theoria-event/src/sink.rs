use std::{
    any::Any,
    fmt,
    rc::{Rc, Weak},
};

type ReceiveFn<E> = dyn Fn(&dyn Any, E);

pub(crate) trait SinkEndpoint: 'static {
    type Event;

    fn receive(&self, event: Self::Event);
}

/// A cloneable typed destination independent of any Dispatcher.
///
/// A Sink does not keep its endpoint alive. Send directly on the current call
/// stack with [`Self::send`], or submit an owned event through a
/// [`crate::Dispatcher`]. If the endpoint has already been dropped when a
/// Dispatch starts, it completes with
/// [`crate::DispatchOutcome::SinkUnavailable`].
pub struct EventSink<E> {
    endpoint: Weak<dyn Any>,
    receive: Rc<ReceiveFn<E>>,
}

impl<E> EventSink<E> {
    pub(crate) fn from_endpoint<T>(endpoint: Weak<T>) -> Self
    where
        T: SinkEndpoint<Event = E>,
    {
        Self {
            endpoint,
            receive: Rc::new(move |endpoint, event| {
                let endpoint = endpoint
                    .downcast_ref::<T>()
                    .expect("an EventSink endpoint must retain its original type");
                endpoint.receive(event);
            }),
        }
    }

    /// Creates a Sink accepting `U` by converting it into this Sink's `E`.
    pub fn contramap<U>(&self, map: impl Fn(U) -> E + 'static) -> EventSink<U>
    where
        E: 'static,
        U: 'static,
    {
        self.contramap_filter(move |event| Some(map(event)))
    }

    /// Creates a Sink that accepts only values satisfying `predicate`.
    ///
    /// For a live endpoint, a filtered-out value completes its Dispatch as
    /// delivered without invoking the endpoint callback. Endpoint liveness is
    /// checked before the predicate runs.
    pub fn filter(&self, predicate: impl Fn(&E) -> bool + 'static) -> EventSink<E>
    where
        E: 'static,
    {
        self.contramap_filter(move |event| predicate(&event).then_some(event))
    }

    /// Creates a Sink accepting `U` by optionally converting it into this
    /// Sink's `E`.
    ///
    /// For a live endpoint, returning `None` completes that Dispatch as
    /// delivered without invoking the endpoint callback. Endpoint liveness is
    /// checked before `map` runs.
    pub fn contramap_filter<U>(&self, map: impl Fn(U) -> Option<E> + 'static) -> EventSink<U>
    where
        E: 'static,
        U: 'static,
    {
        let receive = Rc::clone(&self.receive);
        EventSink {
            endpoint: Weak::clone(&self.endpoint),
            receive: Rc::new(move |endpoint, event| {
                if let Some(event) = map(event) {
                    receive(endpoint, event);
                }
            }),
        }
    }

    /// Creates a Sink accepting every `U: Into<E>`.
    pub fn accepting<U>(&self) -> EventSink<U>
    where
        E: 'static,
        U: Into<E> + 'static,
    {
        self.contramap(Into::into)
    }

    /// Sends one event directly to this Sink on the current call stack.
    ///
    /// Direct delivery does not erase or retain the event and therefore adds
    /// no `E: 'static` requirement. If the endpoint has already been dropped,
    /// ownership of the unconsumed event is returned. Sending from a handler
    /// may synchronously enter another handler; use a Dispatcher when such
    /// work must be serialized.
    pub fn send(&self, event: E) -> Result<(), E> {
        let Some(endpoint) = self.endpoint.upgrade() else {
            return Err(event);
        };
        (self.receive)(endpoint.as_ref(), event);
        Ok(())
    }

    pub(crate) fn is_reachable(&self) -> bool {
        self.endpoint.strong_count() != 0
    }
}

impl<E> Clone for EventSink<E> {
    fn clone(&self) -> Self {
        Self {
            endpoint: Weak::clone(&self.endpoint),
            receive: Rc::clone(&self.receive),
        }
    }
}

impl<E> From<&EventSink<E>> for EventSink<E> {
    fn from(sink: &EventSink<E>) -> Self {
        sink.clone()
    }
}

impl<E> fmt::Debug for EventSink<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("EventSink").finish_non_exhaustive()
    }
}
