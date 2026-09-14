use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    fmt,
    rc::{Rc, Weak},
};

use crate::{
    sink::{EventEndpoint, EventTarget},
    source::{EventSource, Subscription},
};

type Subscriber<E> = Rc<dyn Fn(&E)>;

/// Creates an owning publication endpoint and a non-owning Channel Source.
pub fn channel<E: 'static>() -> (EventProvider<E>, ChannelSource<E>) {
    let state = Rc::new(ChannelState {
        next_subscription: Cell::new(0),
        subscriptions: RefCell::new(BTreeMap::new()),
    });
    (
        EventProvider {
            state: Rc::clone(&state),
        },
        ChannelSource {
            state: Rc::downgrade(&state),
        },
    )
}

/// Owns one typed event channel and accepts values published into it.
///
/// Dropping the Provider closes the channel. Sinks and Source views derived
/// from it do not extend its lifetime.
pub struct EventProvider<E: 'static> {
    state: Rc<ChannelState<E>>,
}

impl<E: 'static> EventProvider<E> {
    pub(crate) fn publish(&self, event: E) {
        self.state.publish(event);
    }
}

impl<E: 'static> From<&EventProvider<E>> for EventTarget<E> {
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

/// A cloneable, non-owning view of one typed channel.
///
/// A Channel Source lends each publication to its subscribers. Connect it to
/// an [`crate::sink_adapter::OwningSink`] when the resulting Dispatch must own
/// the event and outlive the publication call.
pub struct ChannelSource<E: 'static> {
    state: Weak<ChannelState<E>>,
}

impl<E: 'static> Clone for ChannelSource<E> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
        }
    }
}

impl<E: 'static> fmt::Debug for ChannelSource<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ChannelSource")
            .finish_non_exhaustive()
    }
}

impl<E: 'static> EventSource for ChannelSource<E> {
    type Event<'event> = &'event E;

    fn subscribe<F>(self, receive: F) -> Option<Subscription>
    where
        F: for<'event> Fn(Self::Event<'event>) + 'static,
    {
        Some(self.state.upgrade()?.subscribe(move |event| receive(event)))
    }
}

struct ChannelState<E: 'static> {
    next_subscription: Cell<u64>,
    subscriptions: RefCell<BTreeMap<u64, Subscriber<E>>>,
}

impl<E: 'static> ChannelState<E> {
    fn subscribe(self: &Rc<Self>, receive: impl Fn(&E) + 'static) -> Subscription {
        let id = self.next_subscription.get();
        self.next_subscription.set(
            id.checked_add(1)
                .expect("event source subscription identity exhausted"),
        );
        self.subscriptions.borrow_mut().insert(id, Rc::new(receive));

        let state = Rc::downgrade(self);
        Subscription::new(move || {
            if let Some(state) = state.upgrade() {
                state.subscriptions.borrow_mut().remove(&id);
            }
        })
    }

    fn publish(&self, event: E) {
        let subscribers = self
            .subscriptions
            .borrow()
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for receive in subscribers {
            receive(&event);
        }
    }
}

impl<E: 'static> EventEndpoint for ChannelState<E> {
    type Event = E;

    fn receive(&self, event: E) {
        self.publish(event);
    }
}
