//! Thread-level driving of native messages and Window work.

use std::time::{Duration, Instant};

use crate::windows::message_pump::{MessagePumpProgress, WindowsMessagePump};

use super::{Result, Window, WindowProgress};

/// Time an otherwise idle iteration may lend to GPU presentation progress.
const PRESENTATION_WAIT_SLICE: Duration = Duration::from_millis(1);

/// Drives Windows through the Win32 message queue of their GUI thread.
///
/// Keeping the thread message-pump capability here avoids assigning ownership
/// of that shared queue to any individual native Window.
pub(super) struct WindowEventLoop {
    message_pump: WindowsMessagePump,
    windows: Vec<DrivenWindow>,
    next_presentation_wait: usize,
}

struct DrivenWindow {
    window: Window,
    progress: WindowProgress,
}

impl WindowEventLoop {
    pub(super) fn new(windows: impl IntoIterator<Item = Window>) -> Self {
        let windows = windows
            .into_iter()
            .map(|mut window| {
                window.request_redraw_if_needed();
                let progress = if window.has_ready_work() {
                    WindowProgress::Pending
                } else {
                    WindowProgress::Idle
                };
                DrivenWindow { window, progress }
            })
            .collect();
        Self {
            message_pump: WindowsMessagePump::new(),
            windows,
            next_presentation_wait: 0,
        }
    }

    pub(super) fn run(mut self) -> Result<()> {
        while !self.windows.is_empty() {
            let now = Instant::now();
            for window in &mut self.windows {
                window.window.prepare_animation_frame(now);
            }
            let all_idle = self
                .windows
                .iter()
                .all(|window| matches!(window.progress, WindowProgress::Idle));
            let message_progress = if all_idle {
                if let Some(wait) = self.time_until_animation_frame(now) {
                    self.message_pump.wait_for(wait)?
                } else {
                    self.message_pump.wait()?
                }
            } else {
                self.message_pump.poll()
            };
            if matches!(message_progress, MessagePumpProgress::Finished) {
                break;
            }

            let waiting_window = if matches!(message_progress, MessagePumpProgress::Idle) {
                self.next_window_waiting_for_presentation()
            } else {
                None
            };
            if let Some(index) = waiting_window {
                self.next_presentation_wait = index + 1;
            }

            for (index, window) in self.windows.iter_mut().enumerate() {
                let presentation_wait = if Some(index) == waiting_window {
                    PRESENTATION_WAIT_SLICE
                } else {
                    Duration::ZERO
                };
                window.progress = window.window.process_ready_work(presentation_wait)?;
            }
            self.windows
                .retain(|window| !matches!(window.progress, WindowProgress::Finished));
            if !self.windows.is_empty() {
                self.next_presentation_wait %= self.windows.len();
            }
        }
        Ok(())
    }

    fn next_window_waiting_for_presentation(&self) -> Option<usize> {
        let count = self.windows.len();
        (0..count)
            .map(|offset| (self.next_presentation_wait + offset) % count)
            .find(|&index| {
                matches!(
                    self.windows[index].progress,
                    WindowProgress::PresentationPending
                )
            })
    }

    fn time_until_animation_frame(&self, now: Instant) -> Option<Duration> {
        self.windows
            .iter()
            .filter_map(|window| window.window.time_until_animation_frame(now))
            .min()
    }
}
