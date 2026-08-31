use std::{
    any::Any,
    cell::{Cell, RefCell},
    fmt,
    marker::PhantomData,
    rc::{Rc, Weak},
};

use super::{Component, ComponentResource, arena::ComponentId, runtime::ComponentRuntimeInner};

/// A typed event output that can be subscribed to repeatedly.
///
/// A source is deliberately not cloneable: it is the single public identity of
/// one event stream. Borrowing it to add a subscription does not consume it.
/// Holding a source does not keep the component that created it alive.
pub struct EventSource<E: 'static> {
    runtime: Weak<ComponentRuntimeInner>,
    endpoint: Weak<EventEndpoint<E>>,
    marker: PhantomData<fn(E)>,
}

impl<E: 'static> fmt::Debug for EventSource<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EventSource")
            .finish_non_exhaustive()
    }
}

/// Cloneable authority to publish values to an [`EventSource`].
///
/// Cloning an emitter explicitly shares publication authority. Emitters are
/// non-owning and become inactive when the component scope that created their
/// source is unmounted.
pub struct EventEmitter<E: 'static> {
    runtime: Weak<ComponentRuntimeInner>,
    endpoint: Weak<EventEndpoint<E>>,
    marker: PhantomData<fn(E)>,
}

impl<E: 'static> Clone for EventEmitter<E> {
    fn clone(&self) -> Self {
        Self {
            runtime: Weak::clone(&self.runtime),
            endpoint: Weak::clone(&self.endpoint),
            marker: PhantomData,
        }
    }
}

impl<E: 'static> fmt::Debug for EventEmitter<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EventEmitter")
            .finish_non_exhaustive()
    }
}

pub(super) fn event<E: 'static>(
    runtime: &ComponentRuntimeInner,
) -> (EventSource<E>, EventEmitter<E>, Box<dyn ComponentResource>) {
    let endpoint = Rc::new(EventEndpoint::new(runtime.weak()));
    let source = EventSource {
        runtime: runtime.weak(),
        endpoint: Rc::downgrade(&endpoint),
        marker: PhantomData,
    };
    let emitter = EventEmitter {
        runtime: runtime.weak(),
        endpoint: Rc::downgrade(&endpoint),
        marker: PhantomData,
    };
    let owner: Box<dyn ComponentResource> = Box::new(EventOwner { endpoint });
    (source, emitter, owner)
}

impl ComponentRuntimeInner {
    pub(super) fn subscribe<C, E, F>(
        &self,
        source: &EventSource<E>,
        target: &super::ComponentHandle<C>,
        map: F,
    ) -> Option<Box<dyn ComponentResource>>
    where
        C: Component,
        E: 'static,
        F: Fn(&E) -> C::Event + 'static,
    {
        self.assert_event_runtime(&source.runtime, "event source");
        self.assert_handle_runtime(target);
        if !self.accepts_events(target.id()) {
            return None;
        }
        let endpoint = source.endpoint.upgrade()?;
        if !endpoint.active.get() {
            return None;
        }
        let subscription = endpoint.subscribe(target.id(), move |event| Box::new(map(event)));
        Some(Box::new(subscription))
    }

    pub(super) fn emit<E: 'static>(&self, emitter: &EventEmitter<E>, event: E) -> bool {
        self.assert_event_runtime(&emitter.runtime, "event emitter");
        let Some(endpoint) = emitter.endpoint.upgrade() else {
            return false;
        };
        endpoint.emit(self, event)
    }

    fn assert_event_runtime(&self, runtime: &Weak<ComponentRuntimeInner>, kind: &str) {
        let runtime = runtime
            .upgrade()
            .unwrap_or_else(|| panic!("{kind} used after its ComponentRuntime was dropped"));
        assert!(
            std::ptr::eq(runtime.as_ref(), self),
            "{kind} belongs to a different ComponentRuntime"
        );
    }
}

struct EventEndpoint<E: 'static> {
    runtime: Weak<ComponentRuntimeInner>,
    active: Cell<bool>,
    next_subscription: Cell<u64>,
    subscriptions: RefCell<Vec<EventSubscription<E>>>,
}

impl<E: 'static> EventEndpoint<E> {
    fn new(runtime: Weak<ComponentRuntimeInner>) -> Self {
        Self {
            runtime,
            active: Cell::new(true),
            next_subscription: Cell::new(0),
            subscriptions: RefCell::new(Vec::new()),
        }
    }

    fn subscribe(
        self: &Rc<Self>,
        target: ComponentId,
        convert: impl Fn(&E) -> Box<dyn Any> + 'static,
    ) -> SubscriptionLease<E> {
        let id = self.next_subscription.get();
        self.next_subscription.set(
            id.checked_add(1)
                .expect("event source subscription identity exhausted"),
        );
        self.subscriptions.borrow_mut().push(EventSubscription {
            id,
            target,
            convert: Box::new(convert),
        });
        SubscriptionLease {
            endpoint: Rc::downgrade(self),
            id,
        }
    }

    fn emit(&self, runtime: &ComponentRuntimeInner, event: E) -> bool {
        if !self.active.get() {
            return false;
        }
        debug_assert!(
            self.runtime
                .upgrade()
                .is_some_and(|owner| std::ptr::eq(owner.as_ref(), runtime)),
            "event endpoint runtime changed"
        );

        let mut subscriptions = self.subscriptions.borrow_mut();
        subscriptions.retain(|subscription| runtime.accepts_events(subscription.target));
        for subscription in subscriptions.iter() {
            let event = (subscription.convert)(&event);
            let queued = runtime.enqueue_erased(subscription.target, event);
            debug_assert!(queued, "validated event subscriber became unavailable");
        }
        true
    }

    fn remove_subscription(&self, id: u64) {
        self.subscriptions
            .borrow_mut()
            .retain(|subscription| subscription.id != id);
    }

    fn deactivate(&self) {
        self.active.set(false);
        self.subscriptions.borrow_mut().clear();
    }
}

struct EventSubscription<E: 'static> {
    id: u64,
    target: ComponentId,
    convert: Box<EventMapper<E>>,
}

type EventMapper<E> = dyn Fn(&E) -> Box<dyn Any>;

struct EventOwner<E: 'static> {
    endpoint: Rc<EventEndpoint<E>>,
}

impl<E: 'static> ComponentResource for EventOwner<E> {
    fn deactivate(&self) {
        self.endpoint.deactivate();
    }
}

impl<E: 'static> Drop for EventOwner<E> {
    fn drop(&mut self) {
        self.deactivate();
    }
}

struct SubscriptionLease<E: 'static> {
    endpoint: Weak<EventEndpoint<E>>,
    id: u64,
}

impl<E: 'static> ComponentResource for SubscriptionLease<E> {
    fn deactivate(&self) {
        if let Some(endpoint) = self.endpoint.upgrade() {
            endpoint.remove_subscription(self.id);
        }
    }
}

impl<E: 'static> Drop for SubscriptionLease<E> {
    fn drop(&mut self) {
        self.deactivate();
    }
}
