//! Windows platform integration.

use crate::event_loop::EventLoop;

pub(crate) mod message_pump;
mod window;

pub(crate) use window::create_window;

/// Creates an EventLoop backed by the current thread's Win32 message queue.
///
/// The returned coordinator otherwise uses the platform-independent Theoria
/// event and reactive machinery. Constructing it here keeps native loop
/// selection out of the common [`EventLoop`] contract.
pub fn event_loop() -> EventLoop {
    EventLoop::with_pump(Box::new(message_pump::WindowsMessagePump::new()))
}
