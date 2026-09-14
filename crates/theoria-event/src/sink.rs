use std::{
    any::Any,
    fmt,
    rc::{Rc, Weak},
};

use crate::dispatch::{DispatchOutcome, EventDispatch};

type ReceiveFn<E> = dyn Fn(&dyn Any, E);

/// Builds a Dispatch from one event value.
///
/// Implementing this trait for every lifetime in an event family lets one
/// Sink accept scoped events such as native messages without retaining them.
pub trait DispatchSink<E> {
    fn dispatch<'event>(&self, event: E) -> EventDispatch<'event>
    where
        E: 'event;
}

/// A Sink adapter that turns a borrowed input into an owned event.
///
/// This is the borrowing counterpart of [`EventSink::contramap`]. The
/// conversion runs only when the resulting Dispatch runs, while the borrowed
/// input is still valid.
pub struct BorrowingEventSink<T: ?Sized, E> {
    sink: EventSink<E>,
    map: Rc<dyn for<'event> Fn(&'event T) -> E>,
}

pub(crate) trait SinkEndpoint: 'static {
    type Event;

    fn receive(&self, event: Self::Event);
}

/// A cloneable typed destination independent of any Dispatcher.
///
/// A Sink does not keep its endpoint alive. Send directly on the current call
/// stack with [`Self::send`], or submit an owned event through a
/// [`crate::dispatch::Dispatcher`]. If the endpoint has already been dropped when a
/// Dispatch starts, it completes with
/// [`crate::dispatch::DispatchOutcome::SinkUnavailable`].
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

    /// Creates a Sink accepting a borrowed `T` by converting it into `E`.
    pub fn contramap_borrow<T: ?Sized>(
        &self,
        map: impl for<'event> Fn(&'event T) -> E + 'static,
    ) -> BorrowingEventSink<T, E>
    where
        E: 'static,
    {
        BorrowingEventSink {
            sink: self.clone(),
            map: Rc::new(map),
        }
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

    /// Builds one invocation carrying `event` to this Sink.
    ///
    /// Building a Dispatch does not invoke the endpoint. The Dispatch owns the
    /// event and may run on the current call stack. When `E: 'static`, it may
    /// instead be submitted to a Dispatcher.
    pub fn send<'event>(&self, event: E) -> EventDispatch<'event>
    where
        E: 'event,
    {
        let sink = self.clone();
        EventDispatch::new(move || sink.deliver(event))
    }

    /// Delivers one event immediately, returning it when the endpoint is gone.
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

impl<E> DispatchSink<E> for EventSink<E> {
    fn dispatch<'event>(&self, event: E) -> EventDispatch<'event>
    where
        E: 'event,
    {
        self.send(event)
    }
}

impl<'input, T: ?Sized, E> DispatchSink<&'input T> for BorrowingEventSink<T, E>
where
    E: 'static,
{
    fn dispatch<'event>(&self, event: &'input T) -> EventDispatch<'event>
    where
        &'input T: 'event,
    {
        let sink = self.sink.clone();
        let map = Rc::clone(&self.map);
        EventDispatch::new(move || sink.deliver(map(event)))
    }
}

impl<T: ?Sized, E> Clone for BorrowingEventSink<T, E> {
    fn clone(&self) -> Self {
        Self {
            sink: self.sink.clone(),
            map: Rc::clone(&self.map),
        }
    }
}

impl<T: ?Sized, E> fmt::Debug for BorrowingEventSink<T, E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BorrowingEventSink")
            .finish_non_exhaustive()
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
