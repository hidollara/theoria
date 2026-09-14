use std::{
    cell::RefCell,
    fmt,
    rc::{Rc, Weak},
};

use theoria_event::{Dispatch, DispatchOutcome, EventSink};

type ComponentCallback<E> = Box<dyn FnMut(E)>;

/// Owns one event callback retained by a mounted Component.
pub(crate) struct ComponentReceiver<E: 'static> {
    callback: RefCell<ComponentCallback<E>>,
}

impl<E: 'static> ComponentReceiver<E> {
    pub(crate) fn new(callback: impl FnMut(E) + 'static) -> Rc<Self> {
        Rc::new(Self {
            callback: RefCell::new(Box::new(callback)),
        })
    }

    pub(crate) fn sink(self: &Rc<Self>) -> ComponentSink<E> {
        ComponentSink {
            receiver: Rc::downgrade(self),
        }
    }

    fn receive(&self, event: E) {
        let mut callback = self
            .callback
            .try_borrow_mut()
            .expect("a ComponentSink cannot be reentered");
        callback(event);
    }
}

/// A cloneable, non-owning input capability for one Component callback.
///
/// Unmounting the owning Component eventually drops the callback. Dispatches
/// run after that point complete with [`DispatchOutcome::SinkClosed`].
/// Recursively invoking the same callback is a contract violation and panics;
/// use a serializing Dispatcher when re-entry must be deferred.
pub struct ComponentSink<E: 'static> {
    receiver: Weak<ComponentReceiver<E>>,
}

impl<E: 'static> EventSink<E> for ComponentSink<E> {
    type Dispatch = ComponentDispatch<E>;

    fn send(self, event: E) -> Self::Dispatch {
        ComponentDispatch {
            receiver: self.receiver,
            event,
        }
    }
}

impl<E: 'static> EventSink<E> for &ComponentSink<E> {
    type Dispatch = ComponentDispatch<E>;

    fn send(self, event: E) -> Self::Dispatch {
        self.clone().send(event)
    }
}

impl<E: 'static> Clone for ComponentSink<E> {
    fn clone(&self) -> Self {
        Self {
            receiver: self.receiver.clone(),
        }
    }
}

impl<E: 'static> fmt::Debug for ComponentSink<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ComponentSink")
            .finish_non_exhaustive()
    }
}

/// One concrete invocation of a Component callback.
#[must_use = "a ComponentDispatch must be run, submitted, or intentionally discarded"]
pub struct ComponentDispatch<E: 'static> {
    receiver: Weak<ComponentReceiver<E>>,
    event: E,
}

impl<E: 'static> Dispatch for ComponentDispatch<E> {
    fn run(self) -> DispatchOutcome {
        let Some(receiver) = self.receiver.upgrade() else {
            return DispatchOutcome::SinkClosed;
        };
        receiver.receive(self.event);
        DispatchOutcome::Completed
    }
}

impl<E: 'static> fmt::Debug for ComponentDispatch<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ComponentDispatch")
            .finish_non_exhaustive()
    }
}
