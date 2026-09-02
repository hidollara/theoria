use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    fmt,
    rc::{Rc, Weak},
};

use crate::{
    dispatch::{DispatchOutcome, DispatchRoute},
    sink::{EventSink, SinkEndpoint},
};

/// Creates the owning Provider and subscription-only Source of an event channel.
///
/// The two capabilities share one channel but neither can recreate the other.
/// The Provider owns the channel and acts as its publication endpoint; the
/// Source is its cloneable, non-owning subscription interface.
pub fn channel<E: 'static>() -> (EventProvider<E>, EventSource<E>) {
    let state = Rc::new(ChannelState {
        next_subscription: Cell::new(0),
        subscriptions: RefCell::new(BTreeMap::new()),
    });
    (
        EventProvider {
            state: Rc::clone(&state),
        },
        EventSource {
            state: Rc::downgrade(&state),
        },
    )
}

/// Owns one typed event channel and acts as its publication endpoint.
///
/// Borrowing a Provider as an [`EventSink`] creates a non-owning input
/// capability. Dropping the Provider closes its [`EventSource`] and makes all
/// derived Sinks unavailable.
pub struct EventProvider<E: 'static> {
    state: Rc<ChannelState<E>>,
}

impl<E: 'static> EventProvider<E> {
    pub(crate) fn publish(&self, event: E) {
        self.state.publish(event);
    }
}

impl<E: 'static> From<&EventProvider<E>> for EventSink<E> {
    fn from(provider: &EventProvider<E>) -> Self {
        Self::from_endpoint(Rc::downgrade(&provider.state))
    }
}

impl<E: 'static> fmt::Debug for EventProvider<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EventProvider")
            .finish_non_exhaustive()
    }
}

/// A non-owning subscription interface for a typed event channel.
///
/// Every subscription retains its own dispatch route. For each event,
/// the channel captures the current subscriptions and submits their Dispatches
/// in subscription creation order. Subscription changes made by inline
/// execution take effect with the next event.
pub struct EventSource<E: 'static> {
    state: Weak<ChannelState<E>>,
}

impl<E: 'static> fmt::Debug for EventSource<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EventSource")
            .finish_non_exhaustive()
    }
}

impl<E: 'static> EventSource<E> {
    /// Subscribes a Sink using `T::from(&E)` for each event.
    ///
    /// Returns `None` when the EventProvider has already been dropped.
    pub fn subscribe<T>(
        &self,
        sink: impl Into<EventSink<T>>,
        route: &DispatchRoute,
    ) -> Option<EventSubscription<E>>
    where
        T: 'static,
        for<'event> T: From<&'event E>,
    {
        self.subscribe_map(sink, route, |event| T::from(event))
    }

    /// Subscribes by explicitly cloning each event before conversion.
    ///
    /// Returns `None` when the EventProvider has already been dropped.
    pub fn subscribe_cloned<T>(
        &self,
        sink: impl Into<EventSink<T>>,
        route: &DispatchRoute,
    ) -> Option<EventSubscription<E>>
    where
        E: Clone,
        T: From<E> + 'static,
    {
        self.subscribe_map(sink, route, |event| T::from(event.clone()))
    }

    /// Subscribes with an explicit borrowed-value adapter.
    ///
    /// The adapter runs when this subscription's Dispatch executes, not while
    /// the channel expands the publication. If its route rejects submission,
    /// only this subscription's copy of the current event is dropped.
    pub fn subscribe_map<T, F>(
        &self,
        sink: impl Into<EventSink<T>>,
        route: &DispatchRoute,
        map: F,
    ) -> Option<EventSubscription<E>>
    where
        T: 'static,
        F: Fn(&E) -> T + 'static,
    {
        Some(
            self.state
                .upgrade()?
                .subscribe(sink.into(), route.clone(), map),
        )
    }
}

impl<E: 'static> Clone for EventSource<E> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
        }
    }
}

struct ChannelState<E: 'static> {
    next_subscription: Cell<u64>,
    subscriptions: RefCell<BTreeMap<u64, Subscription<E>>>,
}

struct Subscription<E: 'static> {
    sink: EventSink<Rc<E>>,
    route: DispatchRoute,
    removal: SubscriptionRemoval<E>,
}

struct SubscriptionRemoval<E: 'static> {
    channel: Weak<ChannelState<E>>,
    id: u64,
}

impl<E: 'static> SubscriptionRemoval<E> {
    fn remove(&self) {
        if let Some(channel) = self.channel.upgrade() {
            channel.remove_subscription(self.id);
        }
    }
}

impl<E: 'static> Clone for SubscriptionRemoval<E> {
    fn clone(&self) -> Self {
        Self {
            channel: self.channel.clone(),
            id: self.id,
        }
    }
}

impl<E: 'static> ChannelState<E> {
    fn subscribe<T, F>(
        self: &Rc<Self>,
        sink: EventSink<T>,
        route: DispatchRoute,
        map: F,
    ) -> EventSubscription<E>
    where
        T: 'static,
        F: Fn(&E) -> T + 'static,
    {
        let id = self.next_subscription.get();
        self.next_subscription.set(
            id.checked_add(1)
                .expect("event source subscription identity exhausted"),
        );
        let sink = sink.contramap(move |event: Rc<E>| map(event.as_ref()));
        let removal = SubscriptionRemoval {
            channel: Rc::downgrade(self),
            id,
        };
        self.subscriptions.borrow_mut().insert(
            id,
            Subscription {
                sink,
                route,
                removal,
            },
        );
        EventSubscription {
            channel: Some(Rc::downgrade(self)),
            id,
        }
    }

    fn publish(&self, event: E) {
        let event = Rc::new(event);
        let subscriptions = {
            let mut subscriptions = self.subscriptions.borrow_mut();
            subscriptions.retain(|_, subscription| subscription.sink.is_reachable());
            subscriptions
                .values()
                .map(|subscription| {
                    (
                        subscription.sink.clone(),
                        subscription.route.clone(),
                        subscription.removal.clone(),
                    )
                })
                .collect::<Vec<_>>()
        };
        for (sink, route, removal) in subscriptions {
            if let Ok(receipt) = route.send_tracked(sink, Rc::clone(&event)) {
                receipt.on_complete(move |outcome| {
                    if outcome == DispatchOutcome::SinkUnavailable {
                        removal.remove();
                    }
                });
            }
        }
    }

    fn remove_subscription(&self, id: u64) {
        self.subscriptions.borrow_mut().remove(&id);
    }
}

impl<E: 'static> SinkEndpoint for ChannelState<E> {
    type Event = E;

    fn receive(&self, event: E) {
        self.publish(event);
    }
}

/// Owns one subscription from a Source to a Sink.
///
/// It is removed explicitly with [`Self::unsubscribe`] or implicitly on
/// drop. The subscription does not keep the EventProvider alive. Dispatches
/// already submitted for this subscription remain valid after it is removed.
/// [`Self::detach`] leaves the subscription installed until its Provider is
/// dropped. A subscription whose Sink becomes unavailable during Dispatch is
/// removed immediately. An already unavailable Sink is removed before the next
/// publication is expanded.
#[must_use = "dropping an EventSubscription immediately removes the subscription"]
pub struct EventSubscription<E: 'static> {
    channel: Option<Weak<ChannelState<E>>>,
    id: u64,
}

impl<E: 'static> EventSubscription<E> {
    /// Removes this subscription immediately.
    ///
    /// This has the same effect as dropping the subscription, but makes the
    /// intended timing explicit.
    pub fn unsubscribe(mut self) {
        self.remove();
    }

    /// Relinquishes the ability to unsubscribe while leaving the subscription
    /// installed.
    ///
    /// The Provider continues to own the subscription until it is dropped. If
    /// the subscribed Sink becomes unreachable, the Provider removes the
    /// subscription when an outstanding Dispatch observes that state or before
    /// expanding the next publication.
    pub fn detach(mut self) {
        self.channel.take();
    }

    fn remove(&mut self) {
        if let Some(channel) = self.channel.take().and_then(|channel| channel.upgrade()) {
            channel.remove_subscription(self.id);
        }
    }
}

impl<E: 'static> fmt::Debug for EventSubscription<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EventSubscription")
            .finish_non_exhaustive()
    }
}

impl<E: 'static> Drop for EventSubscription<E> {
    fn drop(&mut self) {
        self.remove();
    }
}

#[cfg(test)]
mod tests {
    use super::channel;
    use crate::{
        dispatch::DispatchRoute, dispatcher::FifoDispatcher, handler::EventHandler, sink::EventSink,
    };

    #[test]
    fn sink_unavailability_removes_a_deferred_subscription() {
        let (provider, source) = channel::<u32>();
        let channel = provider.state.clone();
        let mut dispatcher = FifoDispatcher::new();
        let route = dispatcher.route();
        let handler = EventHandler::new(|_: u32| {});
        source
            .subscribe_cloned(EventSink::from(&handler), &route)
            .expect("live channel")
            .detach();

        assert_eq!(channel.subscriptions.borrow().len(), 1);
        assert!(DispatchRoute::Inline.send(&provider, 7).is_ok());
        drop(handler);

        assert!(dispatcher.deliver_next());
        assert!(channel.subscriptions.borrow().is_empty());
    }
}
