use std::fmt;

use crate::{dispatch::Dispatcher, sink::DispatchSink};

/// A stream-like capability whose event type may vary with each publication's
/// lifetime.
pub trait EventSource: Sized {
    type Event<'event>;

    /// Registers one callback for future events.
    ///
    /// Most callers should use [`Self::connect`] and select inline or deferred
    /// execution on the resulting [`EventConnection`].
    fn subscribe<F>(self, receive: F) -> Option<Subscription>
    where
        F: for<'event> Fn(Self::Event<'event>) + 'static;

    /// Pairs this Source with a Sink before selecting how generated Dispatches
    /// are executed.
    fn connect<S>(self, sink: S) -> EventConnection<Self, S> {
        EventConnection { source: self, sink }
    }
}

/// A Source and Sink pair awaiting an execution choice.
pub struct EventConnection<S, K> {
    source: S,
    sink: K,
}

impl<S, K> EventConnection<S, K>
where
    S: EventSource,
    K: 'static,
{
    /// Runs every generated Dispatch inline while its event remains valid.
    pub fn inline(self) -> Option<Subscription>
    where
        for<'event> K: DispatchSink<S::Event<'event>>,
    {
        let Self { source, sink } = self;
        source.subscribe(move |event| {
            let _ = sink.dispatch(event).run();
        })
    }

    /// Submits every generated self-contained Dispatch to `dispatcher`.
    pub fn via(self, dispatcher: impl AsRef<Dispatcher>) -> Option<Subscription>
    where
        for<'event> K: DispatchSink<S::Event<'event>>,
        for<'event> S::Event<'event>: 'static,
    {
        let Self { source, sink } = self;
        let dispatcher = dispatcher.as_ref().clone();
        source.subscribe(move |event| {
            let _ = dispatcher.submit(sink.dispatch(event));
        })
    }
}

impl<S, K> fmt::Debug for EventConnection<S, K> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EventConnection")
            .finish_non_exhaustive()
    }
}

/// Owns one registered relationship between a Source and its subscriber.
///
/// Dropping or explicitly unsubscribing removes the relationship. Detaching
/// leaves it registered and relinquishes the ability to remove it.
#[must_use = "dropping a Subscription immediately removes the subscription"]
pub struct Subscription {
    remove: Option<Box<dyn FnOnce()>>,
}

impl Subscription {
    /// Creates a subscription whose removal runs `unsubscribe` once.
    ///
    /// Source implementations use this to return control of a newly
    /// registered relationship to their caller.
    pub fn new(unsubscribe: impl FnOnce() + 'static) -> Self {
        Self {
            remove: Some(Box::new(unsubscribe)),
        }
    }

    /// Removes this subscription immediately.
    pub fn unsubscribe(mut self) {
        self.remove();
    }

    /// Leaves the subscription installed and relinquishes removal control.
    pub fn detach(mut self) {
        self.remove.take();
    }

    fn remove(&mut self) {
        if let Some(remove) = self.remove.take() {
            remove();
        }
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.remove();
    }
}

impl fmt::Debug for Subscription {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Subscription")
            .finish_non_exhaustive()
    }
}
