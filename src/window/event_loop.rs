//! Thread-level driving of native messages and Window work.

use crate::windows::message_pump::{MessagePumpProgress, WindowsMessagePump};

use super::{Result, Window, WindowProgress};

/// Drives one Window through the Win32 message queue of its GUI thread.
///
/// `Window::run` is currently a single-Window convenience API. Keeping the
/// thread message-pump capability here avoids assigning ownership of that
/// queue to the native Window itself.
pub(super) struct WindowEventLoop {
    message_pump: WindowsMessagePump,
    window: Window,
    work_pending: bool,
}

impl WindowEventLoop {
    pub(super) fn new(mut window: Window) -> Self {
        window.request_redraw_if_needed();
        let work_pending = window.has_ready_work();
        Self {
            message_pump: WindowsMessagePump::new(),
            window,
            work_pending,
        }
    }

    pub(super) fn run(mut self) -> Result<()> {
        loop {
            let message_progress = if self.work_pending {
                self.message_pump.poll()
            } else {
                self.message_pump.wait()?
            };
            if matches!(message_progress, MessagePumpProgress::Finished) {
                break;
            }

            self.work_pending = match self.window.process_ready_work()? {
                WindowProgress::Idle => false,
                WindowProgress::Pending => true,
                WindowProgress::Finished => break,
            };
        }
        Ok(())
    }
}
