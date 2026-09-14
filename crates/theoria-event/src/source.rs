use std::fmt;

use crate::{
    dispatch::{Dispatch, Dispatcher},
    sink::EventSink,
};

/// A stream-like capability whose event type may vary with each publication's
/// lifetime.
pub trait EventSource: Sized {
    type Event<'event>;

    /// Registers one callback for future events and returns its removal operation.
    ///
    /// Dropping the returned operation leaves the callback registered. Call it
    /// directly to unregister, or let [`Subscription`] own it for RAII cleanup.
    fn register<F>(self, receive: F) -> Option<impl FnOnce() + 'static>
    where
        F: for<'event> Fn(Self::Event<'event>) + 'static;
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
    /// Owns an unregister operation as a Subscription.
    ///
    /// Unlike the bare unregister operation returned by [`EventSource::register`],
    /// dropping the resulting Subscription unregisters it.
    pub fn new(unsubscribe: impl FnOnce() + 'static) -> Self {
        Self {
            remove: Some(Box::new(unsubscribe)),
        }
    }

    /// Starts building a Subscription between `source` and `sink`.
    pub fn builder<S, K>(source: S, sink: K) -> SubscriptionBuilder<S, K>
    where
        S: EventSource,
    {
        SubscriptionBuilder { source, sink }
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

/// Builds one Subscription from a Source, a sink, and a dispatch strategy.
///
/// Calling [`Subscription::builder`] only creates this inert description. The
/// registration is not installed until [`Self::inline`] or [`Self::via`] is
/// called.
pub struct SubscriptionBuilder<S, K> {
    source: S,
    sink: K,
}

impl<S, K> SubscriptionBuilder<S, K>
where
    S: EventSource,
    K: 'static,
{
    /// Owns each borrowed publication before constructing the sink's Dispatch.
    ///
    /// This lets the final Dispatch outlive the Source's publication callback
    /// when the borrowed event implements [`ToOwned`].
    pub fn owned(self) -> OwnedSubscriptionBuilder<S, K> {
        let Self { source, sink } = self;
        OwnedSubscriptionBuilder { source, sink }
    }

    /// Runs every generated Dispatch inline while its event remains valid.
    pub fn inline(self) -> Option<Subscription>
    where
        for<'sink, 'event> &'sink K: EventSink<S::Event<'event>>,
    {
        let Self { source, sink } = self;
        let unregister = source.register(move |event| {
            let _ = EventSink::send(&sink, event).run();
        })?;
        Some(Subscription::new(unregister))
    }

    /// Submits every generated Dispatch to `dispatcher`.
    ///
    /// The selected Dispatcher determines which Dispatch lifetimes and
    /// capabilities it accepts.
    pub fn via<D>(self, dispatcher: D) -> Option<Subscription>
    where
        D: 'static,
        for<'sink, 'event> &'sink K: EventSink<S::Event<'event>>,
        for<'sink, 'event> D: Dispatcher<<&'sink K as EventSink<S::Event<'event>>>::Dispatch>,
    {
        let Self { source, sink } = self;
        let unregister = source.register(move |event| {
            let _ = Dispatcher::submit(&dispatcher, EventSink::send(&sink, event));
        })?;
        Some(Subscription::new(unregister))
    }
}

impl<S, K> fmt::Debug for SubscriptionBuilder<S, K> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SubscriptionBuilder")
            .finish_non_exhaustive()
    }
}

/// Builds one Subscription that owns each borrowed publication before dispatch.
pub struct OwnedSubscriptionBuilder<S, K> {
    source: S,
    sink: K,
}

impl<S, K> OwnedSubscriptionBuilder<S, K>
where
    S: EventSource,
    K: 'static,
{
    /// Runs every generated Dispatch inline after owning its publication.
    pub fn inline<T>(self) -> Option<Subscription>
    where
        T: ToOwned + ?Sized,
        for<'event> S: EventSource<Event<'event> = &'event T>,
        for<'sink> &'sink K: EventSink<T::Owned>,
    {
        let Self { source, sink } = self;
        let unregister = source.register(move |event| {
            let _ = EventSink::send(&sink, event.to_owned()).run();
        })?;
        Some(Subscription::new(unregister))
    }

    /// Submits every generated owned Dispatch to `dispatcher`.
    pub fn via<T, D>(self, dispatcher: D) -> Option<Subscription>
    where
        T: ToOwned + ?Sized,
        D: 'static,
        for<'event> S: EventSource<Event<'event> = &'event T>,
        for<'sink> &'sink K: EventSink<T::Owned>,
        for<'sink> D: Dispatcher<<&'sink K as EventSink<T::Owned>>::Dispatch>,
    {
        let Self { source, sink } = self;
        let unregister = source.register(move |event| {
            let _ = Dispatcher::submit(&dispatcher, EventSink::send(&sink, event.to_owned()));
        })?;
        Some(Subscription::new(unregister))
    }
}

impl<S, K> fmt::Debug for OwnedSubscriptionBuilder<S, K> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OwnedSubscriptionBuilder")
            .finish_non_exhaustive()
    }
}
