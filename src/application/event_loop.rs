//! Thread-level driving of one multi-Window Application.

use std::time::{Duration, Instant};

use crate::{
    application::{Application, Result},
    drive::{COMPONENT_WORK_SLICE, ComponentProgress, PRESENTATION_WAIT_SLICE, drive_components},
    windows::message_pump::{MessagePumpProgress, WindowsMessagePump},
};

pub(super) struct ApplicationEventLoop {
    application: Application,
    message_pump: WindowsMessagePump,
    component_work_pending: bool,
    next_presentation_wait: usize,
}

impl ApplicationEventLoop {
    pub(super) fn new(mut application: Application) -> Self {
        for window in &mut application.windows {
            window.request_redraw_if_needed();
        }
        Self {
            application,
            message_pump: WindowsMessagePump::new(),
            component_work_pending: false,
            next_presentation_wait: 0,
        }
    }

    pub(super) fn run(mut self) -> Result<()> {
        while !self.application.windows.is_empty() && !self.application.components.is_finished() {
            let now = Instant::now();
            for window in &mut self.application.windows {
                window.prepare_animation_frame(now);
            }
            let message_progress = if self.is_idle() {
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

            let now = Instant::now();
            for window in &mut self.application.windows {
                window.process_native_event()?;
                window.publish_due_animation_frame(now);
            }

            self.component_work_pending = match drive_components(
                &mut self.application.dispatcher,
                &mut self.application.components,
                COMPONENT_WORK_SLICE,
            )? {
                ComponentProgress::Idle => false,
                ComponentProgress::Pending => true,
                ComponentProgress::Finished => break,
            };

            if !self.component_work_pending {
                for window in self
                    .application
                    .windows
                    .iter()
                    .filter(|window| window.is_closing())
                {
                    window.unmount();
                }
                self.application
                    .windows
                    .retain(|window| !window.is_closing());
                if self.application.windows.is_empty() {
                    break;
                }
            }

            let waiting_window = if matches!(message_progress, MessagePumpProgress::Idle) {
                self.next_window_waiting_for_presentation()
            } else {
                None
            };
            if let Some(index) = waiting_window {
                self.next_presentation_wait = index + 1;
            }

            for (index, window) in self.application.windows.iter_mut().enumerate() {
                let wait = if Some(index) == waiting_window {
                    PRESENTATION_WAIT_SLICE
                } else {
                    Duration::ZERO
                };
                window.advance_presentation(wait)?;
            }
            self.next_presentation_wait %= self.application.windows.len();
        }
        Ok(())
    }

    fn is_idle(&self) -> bool {
        !self.component_work_pending
            && self
                .application
                .windows
                .iter()
                .all(|window| !window.has_ready_work() && !window.has_pending_presentation())
    }

    fn next_window_waiting_for_presentation(&self) -> Option<usize> {
        let count = self.application.windows.len();
        (0..count)
            .map(|offset| (self.next_presentation_wait + offset) % count)
            .find(|&index| self.application.windows[index].has_pending_presentation())
    }

    fn time_until_animation_frame(&self, now: Instant) -> Option<Duration> {
        self.application
            .windows
            .iter()
            .filter_map(|window| window.time_until_animation_frame(now))
            .min()
    }
}
