//! Borrowed observation of messages delivered to one Win32 Window procedure.

use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    fmt,
    rc::{Rc, Weak},
};

use ::windows::Win32::Foundation::{HWND, LPARAM, WPARAM};

use crate::event::EventSource;
use crate::windows::message::Win32Message;

/// One message observed while its Win32 Window procedure is executing.
///
/// The contained message and any native storage it references are only valid
/// for the callback that receives this value. A subscription therefore has to
/// handle it inline. It may explicitly copy or otherwise own the parts that a
/// Win32 message contract permits it to retain.
#[derive(Clone, Copy, Debug)]
pub struct Win32WindowMessage<'message> {
    window: HWND,
    message: Win32Message<'message>,
}

impl<'message> Win32WindowMessage<'message> {
    const fn new(window: HWND, message: Win32Message<'message>) -> Self {
        Self { window, message }
    }

    /// Returns the Window that received this message.
    pub const fn window(&self) -> HWND {
        self.window
    }

    /// Returns the destination-independent message contents.
    pub const fn message(&self) -> &Win32Message<'message> {
        &self.message
    }
}

/// A cloneable, non-owning Source of messages delivered to one Win32 Window.
///
/// Its events borrow the active Window-procedure call and can consequently
/// only be connected inline. Dropping the Window closes this Source.
pub struct Win32WindowMessageSource {
    state: Weak<WindowMessageState>,
}

impl Clone for Win32WindowMessageSource {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
        }
    }
}

impl fmt::Debug for Win32WindowMessageSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Win32WindowMessageSource")
            .finish_non_exhaustive()
    }
}

impl EventSource for Win32WindowMessageSource {
    type Event<'message> = Win32WindowMessage<'message>;

    fn register<F>(self, receive: F) -> Option<impl FnOnce() + 'static>
    where
        F: for<'message> Fn(Self::Event<'message>) + 'static,
    {
        let state = self.state.upgrade()?;
        let id = state.register(receive);
        let state = Rc::downgrade(&state);
        Some(move || {
            if let Some(state) = state.upgrade() {
                state.unregister(id);
            }
        })
    }
}

/// Owns and publishes the message subscriptions for one native Window.
#[derive(Clone)]
pub(super) struct WindowMessages {
    state: Rc<WindowMessageState>,
}

impl WindowMessages {
    pub(super) fn new() -> Self {
        Self {
            state: Rc::new(WindowMessageState {
                next_subscription: Cell::new(0),
                subscriptions: RefCell::new(BTreeMap::new()),
            }),
        }
    }

    pub(super) fn source(&self) -> Win32WindowMessageSource {
        Win32WindowMessageSource {
            state: Rc::downgrade(&self.state),
        }
    }

    pub(super) fn observe(
        &self,
        receive: impl for<'message> Fn(Win32WindowMessage<'message>) + 'static,
    ) {
        self.state.register(receive);
    }

    pub(super) fn publish(&self, hwnd: HWND, id: u32, wparam: WPARAM, lparam: LPARAM) {
        let lifetime = ();
        let message =
            Win32WindowMessage::new(hwnd, Win32Message::new(id, wparam, lparam, &lifetime));
        let subscribers = self
            .state
            .subscriptions
            .borrow()
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for receive in subscribers {
            receive(message);
        }
    }
}

struct WindowMessageState {
    next_subscription: Cell<u64>,
    #[allow(clippy::type_complexity)]
    subscriptions: RefCell<BTreeMap<u64, Rc<dyn for<'message> Fn(Win32WindowMessage<'message>)>>>,
}

impl WindowMessageState {
    fn register<F>(&self, receive: F) -> u64
    where
        F: for<'message> Fn(Win32WindowMessage<'message>) + 'static,
    {
        let id = self.next_subscription.get();
        self.next_subscription.set(
            id.checked_add(1)
                .expect("Win32 Window message subscription identity exhausted"),
        );
        self.subscriptions.borrow_mut().insert(id, Rc::new(receive));

        id
    }

    fn unregister(&self, id: u64) {
        self.subscriptions.borrow_mut().remove(&id);
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use ::windows::Win32::Foundation::{HWND, LPARAM, WPARAM};

    use crate::{
        component::ComponentInstance,
        event::{DispatchQueue, EventSink, EventSource},
    };

    use super::{Win32WindowMessage, WindowMessages};

    #[test]
    fn source_lends_each_native_message_to_registered_observers() {
        let messages = WindowMessages::new();
        let observed = Rc::new(Cell::new(None));
        let output = Rc::clone(&observed);
        let unregister = messages
            .source()
            .register(move |message| output.set(Some(message.message().id())))
            .expect("the owning Window message source is alive");

        messages.publish(HWND::default(), 42, WPARAM(11), LPARAM(13));

        assert_eq!(observed.get(), Some(42));
        unregister();
        messages.publish(HWND::default(), 84, WPARAM(0), LPARAM(0));
        assert_eq!(observed.get(), Some(42));
    }

    #[test]
    fn source_closes_with_its_owning_window_messages() {
        let messages = WindowMessages::new();
        let source = messages.source();
        let unregister = source
            .clone()
            .register(|_| {})
            .expect("the owning Window message source is alive");
        drop(messages);

        assert!(source.register(|_| {}).is_none());
        unregister();
    }

    #[test]
    fn an_inline_component_connection_can_interpret_a_borrowed_message() {
        let messages = WindowMessages::new();
        let source = messages.source();
        let observed = Rc::new(Cell::new(None));
        let output = Rc::clone(&observed);
        let queue = DispatchQueue::new();
        let _components = ComponentInstance::new(queue.dispatcher(), move |context| {
            let handler = context.receive(move |id: u32, _| output.set(Some(id)));
            let message_id =
                handler.contramap(|message: Win32WindowMessage<'_>| message.message().id());
            assert!(context.connect_inline(source, message_id));
        });

        messages.publish(HWND::default(), 42, WPARAM(0), LPARAM(0));

        assert_eq!(observed.get(), Some(42));
    }
}
