//! Windows platform integration and concrete Window composition.

use std::error::Error;

use crate::event_loop::EventLoop;

mod capabilities;
mod composition;
mod event_buffer;
mod group;
mod host;
mod message_pump;
mod native_window;
mod presentation;
mod presentation_target;
mod window;

pub use self::capabilities::WindowCapabilities;
pub use self::group::{WindowGroup, WindowGroupContext};
pub use self::presentation_target::Win32PresentationTarget;
pub use self::window::Window;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// Creates an EventLoop backed by the current thread's Win32 message queue.
///
/// The returned coordinator otherwise uses the platform-independent Theoria
/// event and reactive machinery. Constructing it here keeps native loop
/// selection out of the common [`EventLoop`] contract.
pub fn event_loop() -> EventLoop {
    EventLoop::with_pump(Box::new(message_pump::WindowsMessagePump::new()))
}
