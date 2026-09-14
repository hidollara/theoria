use std::{cell::RefCell, fmt, rc::Rc};

use crate::sink::{EventEndpoint, EventTarget};

/// Uniquely owns a terminal event handler and acts as its endpoint.
///
/// Borrowing a Handler as an [`EventTarget`] creates a non-owning input
/// capability. Dropping the Handler prevents pending Dispatches for any
/// derived Target from invoking its callback. Recursively sending to the same
/// Handler while its callback is running is a contract violation and panics;
/// use a serializing Dispatcher when re-entry must be deferred.
pub struct EventHandler<E: 'static> {
    state: Rc<HandlerState<E>>,
}

impl<E: 'static> EventHandler<E> {
    /// Creates a Handler that owns the callback and its lifetime.
    pub fn new(handler: impl FnMut(E) + 'static) -> Self {
        let state = Rc::new(HandlerState {
            handler: RefCell::new(Box::new(handler)),
        });
        Self { state }
    }
}

impl<E: 'static> From<&EventHandler<E>> for EventTarget<E> {
    fn from(handler: &EventHandler<E>) -> Self {
        Self::from_endpoint(Rc::downgrade(&handler.state))
    }
}

impl<E: 'static> fmt::Debug for EventHandler<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EventHandler")
            .finish_non_exhaustive()
    }
}

struct HandlerState<E: 'static> {
    handler: RefCell<HandlerCallback<E>>,
}

type HandlerCallback<E> = Box<dyn FnMut(E)>;

impl<E: 'static> HandlerState<E> {
    fn handle(&self, event: E) {
        let mut handler = self
            .handler
            .try_borrow_mut()
            .expect("an EventHandler cannot be reentered");
        handler(event);
    }
}

impl<E: 'static> EventEndpoint for HandlerState<E> {
    type Event = E;

    fn receive(&self, event: E) {
        self.handle(event);
    }
}
