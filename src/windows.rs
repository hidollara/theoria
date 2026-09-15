//! Windows platform integration and concrete Window composition.

use std::{error::Error, fmt};

use crate::event_loop::{EventLoop, EventLoopHandle, EventLoopStatus, EventLoopWaker};

mod capabilities;
mod composition;
mod event_buffer;
mod group;
mod host;
mod message;
mod message_pump;
mod native_window;
mod presentation;
mod presentation_target;
mod window;

pub use self::capabilities::WindowCapabilities;
pub use self::group::{WindowGroup, WindowGroupContext};
pub use self::message::{Win32Message, Win32QueuedMessage, Win32ThreadMessage};
pub use self::message_pump::{Win32QueuedMessageSource, Win32ThreadMessageSource};
pub use self::native_window::{Win32WindowMessage, Win32WindowMessageSource};
pub use self::presentation_target::Win32PresentationTarget;
pub use self::window::Window;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// Owns Theoria coordination and the Win32 message pump for one GUI thread.
pub struct Win32EventLoop {
    event_loop: EventLoop,
    queued_messages: Win32QueuedMessageSource,
    thread_messages: Win32ThreadMessageSource,
}

impl Win32EventLoop {
    fn new() -> Self {
        let pump = message_pump::Win32MessagePump::new();
        let queued_messages = pump.queued_messages();
        let thread_messages = pump.thread_messages();
        Self {
            event_loop: EventLoop::with_pump(Box::new(pump)),
            queued_messages,
            thread_messages,
        }
    }

    /// Returns a non-owning capability for registering work with this loop.
    pub fn handle(&self) -> EventLoopHandle {
        self.event_loop.handle()
    }

    /// Returns a thread-safe, non-owning capability for interrupting this loop.
    pub fn waker(&self) -> EventLoopWaker {
        self.event_loop.waker()
    }

    /// Returns messages removed from this thread's Win32 message queue.
    pub const fn queued_messages(&self) -> &Win32QueuedMessageSource {
        &self.queued_messages
    }

    /// Returns queued messages addressed to this thread rather than a Window.
    pub const fn thread_messages(&self) -> &Win32ThreadMessageSource {
        &self.thread_messages
    }

    /// Drives native messages and registered work until the loop finishes.
    pub fn run(&mut self) -> Result<()> {
        self.event_loop.run()
    }

    /// Performs one driving step, waiting when every participant is idle.
    pub fn step(&mut self) -> Result<EventLoopStatus> {
        self.event_loop.step()
    }

    /// Performs one driving step without intentionally waiting for work.
    pub fn poll(&mut self) -> Result<EventLoopStatus> {
        self.event_loop.poll()
    }
}

impl fmt::Debug for Win32EventLoop {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Win32EventLoop")
            .finish_non_exhaustive()
    }
}

/// Creates a Win32 EventLoop for the current thread.
///
/// Its coordinator uses the platform-independent [`EventLoop`] contract while
/// the wrapper exposes observations specific to the selected Win32 pump.
pub fn event_loop() -> Win32EventLoop {
    Win32EventLoop::new()
}

#[cfg(test)]
mod tests {
    use crate::event::EventSource;

    use super::event_loop;

    #[test]
    fn dropping_a_win32_event_loop_closes_its_message_sources() {
        let event_loop = event_loop();
        let queued_messages = event_loop.queued_messages().clone();
        let thread_messages = event_loop.thread_messages().clone();

        drop(event_loop);

        assert!(queued_messages.register(|_| {}).is_none());
        assert!(thread_messages.register(|_| {}).is_none());
    }
}
