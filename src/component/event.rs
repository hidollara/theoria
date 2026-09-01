use std::{
    any::Any,
    cell::{Cell, RefCell},
    fmt,
    marker::PhantomData,
    rc::{Rc, Weak},
};

use super::{Component, ComponentResource, arena::ComponentId, runtime::ComponentRuntimeInner};

/// A typed event output that can be connected to repeatedly.
///
/// A source is deliberately not cloneable: it is the single public identity of
/// one event stream. Borrowing it to add a connection does not consume it.
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
    pub(super) fn connect<C, E, F>(
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
        let connection = endpoint.connect(target.id(), move |event| Box::new(map(event)));
        Some(Box::new(connection))
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
    next_connection: Cell<u64>,
    connections: RefCell<Vec<EventConnection<E>>>,
}

impl<E: 'static> EventEndpoint<E> {
    fn new(runtime: Weak<ComponentRuntimeInner>) -> Self {
        Self {
            runtime,
            active: Cell::new(true),
            next_connection: Cell::new(0),
            connections: RefCell::new(Vec::new()),
        }
    }

    fn connect(
        self: &Rc<Self>,
        target: ComponentId,
        convert: impl Fn(&E) -> Box<dyn Any> + 'static,
    ) -> ConnectionLease<E> {
        let id = self.next_connection.get();
        self.next_connection.set(
            id.checked_add(1)
                .expect("event source connection identity exhausted"),
        );
        self.connections.borrow_mut().push(EventConnection {
            id,
            target,
            convert: Box::new(convert),
        });
        ConnectionLease {
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

        let mut connections = self.connections.borrow_mut();
        connections.retain(|connection| runtime.accepts_events(connection.target));
        for connection in connections.iter() {
            let event = (connection.convert)(&event);
            let queued = runtime.enqueue_erased(connection.target, event);
            debug_assert!(queued, "validated event target became unavailable");
        }
        true
    }

    fn remove_connection(&self, id: u64) {
        self.connections
            .borrow_mut()
            .retain(|connection| connection.id != id);
    }

    fn deactivate(&self) {
        self.active.set(false);
        self.connections.borrow_mut().clear();
    }
}

struct EventConnection<E: 'static> {
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

struct ConnectionLease<E: 'static> {
    endpoint: Weak<EventEndpoint<E>>,
    id: u64,
}

impl<E: 'static> ComponentResource for ConnectionLease<E> {
    fn deactivate(&self) {
        if let Some(endpoint) = self.endpoint.upgrade() {
            endpoint.remove_connection(self.id);
        }
    }
}

impl<E: 'static> Drop for ConnectionLease<E> {
    fn drop(&mut self) {
        self.deactivate();
    }
}
