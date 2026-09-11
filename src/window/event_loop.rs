//! Thread-level driving of native messages and Window work.

use std::time::Duration;

use crate::windows::message_pump::{MessagePumpProgress, WindowsMessagePump};

use super::{Result, Window, WindowProgress};

/// Time an otherwise idle iteration may lend to GPU presentation progress.
const PRESENTATION_WAIT_SLICE: Duration = Duration::from_millis(1);

/// Drives one Window through the Win32 message queue of its GUI thread.
///
/// `Window::run` is currently a single-Window convenience API. Keeping the
/// thread message-pump capability here avoids assigning ownership of that
/// queue to the native Window itself.
pub(super) struct WindowEventLoop {
    message_pump: WindowsMessagePump,
    window: Window,
    progress: WindowProgress,
}

impl WindowEventLoop {
    pub(super) fn new(mut window: Window) -> Self {
        window.request_redraw_if_needed();
        let progress = if window.has_ready_work() {
            WindowProgress::Pending
        } else {
            WindowProgress::Idle
        };
        Self {
            message_pump: WindowsMessagePump::new(),
            window,
            progress,
        }
    }

    pub(super) fn run(mut self) -> Result<()> {
        loop {
            let message_progress = match self.progress {
                WindowProgress::Idle => self.message_pump.wait()?,
                WindowProgress::Pending | WindowProgress::PresentationPending => {
                    self.message_pump.poll()
                }
                WindowProgress::Finished => break,
            };
            if matches!(message_progress, MessagePumpProgress::Finished) {
                break;
            }

            let presentation_wait = if matches!(
                (self.progress, message_progress),
                (
                    WindowProgress::PresentationPending,
                    MessagePumpProgress::Idle
                )
            ) {
                PRESENTATION_WAIT_SLICE
            } else {
                Duration::ZERO
            };

            self.progress = self.window.process_ready_work(presentation_wait)?;
        }
        Ok(())
    }
}
