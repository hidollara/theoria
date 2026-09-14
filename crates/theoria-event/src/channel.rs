use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    fmt,
    rc::{Rc, Weak},
};

use crate::{
    dispatch::{Dispatch, DispatchOutcome},
    sink::EventSink,
    source::EventSource,
};

/// Owns one typed event channel and creates non-owning capabilities for it.
///
/// Dropping the Channel closes it. Sinks and Source views derived
/// from it do not extend its lifetime.
pub struct Channel<E: 'static> {
    state: Rc<ChannelState<E>>,
}

impl<E: 'static> Channel<E> {
    /// Creates one owned typed event Channel.
    pub fn new() -> Self {
        Self {
            state: Rc::new(ChannelState {
                next_subscription: Cell::new(0),
                subscriptions: RefCell::new(BTreeMap::new()),
            }),
        }
    }

    /// Returns a non-owning subscription capability for this Channel.
    pub fn source(&self) -> ChannelSource<E> {
        ChannelSource {
            state: Rc::downgrade(&self.state),
        }
    }

    /// Returns a non-owning capability for publishing into this channel.
    pub fn sink(&self) -> ChannelSink<E> {
        ChannelSink {
            state: Rc::downgrade(&self.state),
        }
    }
}

impl<E: 'static> Default for Channel<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E: 'static> fmt::Debug for Channel<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("Channel").finish_non_exhaustive()
    }
}

/// A cloneable, non-owning publishing capability for one typed channel.
///
/// Dropping the corresponding [`Channel`] closes this sink. Pending
/// Dispatches then complete with [`DispatchOutcome::SinkClosed`].
pub struct ChannelSink<E: 'static> {
    state: Weak<ChannelState<E>>,
}

impl<E: 'static> EventSink<E> for ChannelSink<E> {
    type Dispatch = ChannelDispatch<E>;

    fn send(self, event: E) -> Self::Dispatch {
        ChannelDispatch {
            state: self.state,
            event,
        }
    }
}

impl<E: 'static> EventSink<E> for &ChannelSink<E> {
    type Dispatch = ChannelDispatch<E>;

    fn send(self, event: E) -> Self::Dispatch {
        self.clone().send(event)
    }
}

impl<E: 'static> Clone for ChannelSink<E> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
        }
    }
}

impl<E: 'static> fmt::Debug for ChannelSink<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ChannelSink")
            .finish_non_exhaustive()
    }
}

/// One concrete publication into a typed channel.
#[must_use = "a ChannelDispatch must be run, submitted, or intentionally discarded"]
pub struct ChannelDispatch<E: 'static> {
    state: Weak<ChannelState<E>>,
    event: E,
}

impl<E: 'static> Dispatch for ChannelDispatch<E> {
    fn run(self) -> DispatchOutcome {
        let Some(state) = self.state.upgrade() else {
            return DispatchOutcome::SinkClosed;
        };
        state.publish(self.event);
        DispatchOutcome::Completed
    }
}

impl<E: 'static> fmt::Debug for ChannelDispatch<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ChannelDispatch")
            .finish_non_exhaustive()
    }
}

/// A cloneable, non-owning view of one typed channel.
///
/// A Channel Source lends each publication to its subscribers. Use
/// [`crate::source::SubscriptionBuilder::owned`] when the resulting Dispatch
/// must own the event and outlive the publication call.
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

    fn register<F>(self, receive: F) -> Option<impl FnOnce() + 'static>
    where
        F: for<'event> Fn(Self::Event<'event>) + 'static,
    {
        Some(self.state.upgrade()?.register(move |event| receive(event)))
    }
}

struct ChannelState<E: 'static> {
    next_subscription: Cell<u64>,
    #[allow(clippy::type_complexity)]
    subscriptions: RefCell<BTreeMap<u64, Rc<dyn Fn(&E)>>>,
}

impl<E: 'static> ChannelState<E> {
    fn register<F>(self: &Rc<Self>, receive: F) -> impl FnOnce() + 'static + use<E, F>
    where
        F: Fn(&E) + 'static,
    {
        let id = self.next_subscription.get();
        self.next_subscription.set(
            id.checked_add(1)
                .expect("event source subscription identity exhausted"),
        );
        self.subscriptions.borrow_mut().insert(id, Rc::new(receive));

        let state = Rc::downgrade(self);
        move || {
            if let Some(state) = state.upgrade() {
                state.subscriptions.borrow_mut().remove(&id);
            }
        }
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
