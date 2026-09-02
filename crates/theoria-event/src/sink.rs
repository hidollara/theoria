use std::{
    any::Any,
    fmt,
    rc::{Rc, Weak},
};

type ReceiveFn<E> = dyn Fn(&dyn Any, E) -> SinkOutcome;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SinkOutcome {
    Delivered,
    Unavailable,
}

pub(crate) trait SinkEndpoint: 'static {
    type Event;

    fn receive(&self, event: Self::Event);
}

/// A cloneable typed destination independent of any dispatch route.
///
/// A Sink does not keep its endpoint alive. Send an event through a
/// [`crate::DispatchRoute`] or a higher-level context that supplies one. If
/// the endpoint has already been dropped when execution starts, dispatch
/// completes with [`crate::DispatchOutcome::SinkUnavailable`].
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
                SinkOutcome::Delivered
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
                    receive(endpoint, event)
                } else {
                    SinkOutcome::Delivered
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

    pub(crate) fn receive(&self, event: E) -> SinkOutcome {
        let Some(endpoint) = self.endpoint.upgrade() else {
            return SinkOutcome::Unavailable;
        };
        (self.receive)(endpoint.as_ref(), event)
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
