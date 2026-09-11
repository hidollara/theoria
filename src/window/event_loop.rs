//! Thread-level driving of native messages, Component work, and presentation.

use std::time::{Duration, Instant};

use crate::{
    component::ComponentInstance,
    drive::{COMPONENT_WORK_SLICE, ComponentProgress, PRESENTATION_WAIT_SLICE, drive_components},
    event::FifoDispatcher,
    windows::message_pump::{MessagePumpProgress, WindowsMessagePump},
};

use super::{Result, WindowHost};

/// One Component tree and the native Windows sharing its update boundary.
pub(crate) struct WindowGroup {
    dispatcher: FifoDispatcher,
    components: ComponentInstance,
    windows: Vec<WindowHost>,
    component_work_pending: bool,
}

impl WindowGroup {
    pub(crate) fn new(
        dispatcher: FifoDispatcher,
        components: ComponentInstance,
        windows: impl IntoIterator<Item = WindowHost>,
    ) -> Self {
        let mut windows: Vec<_> = windows.into_iter().collect();
        for window in &mut windows {
            window.request_redraw_if_needed();
        }
        Self {
            dispatcher,
            components,
            windows,
            // Construction may have submitted deferred events. Start by
            // polling rather than sleeping without inspecting the queue.
            component_work_pending: true,
        }
    }

    fn prepare_animation_frames(&mut self, now: Instant) {
        for window in &mut self.windows {
            window.prepare_animation_frame(now);
        }
    }

    fn time_until_animation_frame(&self, now: Instant) -> Option<Duration> {
        self.windows
            .iter()
            .filter_map(|window| window.time_until_animation_frame(now))
            .min()
    }

    fn is_idle(&self) -> bool {
        !self.component_work_pending
            && self
                .windows
                .iter()
                .all(|window| !window.has_ready_work() && !window.has_pending_presentation())
    }

    fn process_ready_work(&mut self, now: Instant) -> Result<bool> {
        for window in &mut self.windows {
            window.process_native_event()?;
            window.publish_due_animation_frame(now);
        }

        self.component_work_pending = match drive_components(
            &mut self.dispatcher,
            &mut self.components,
            COMPONENT_WORK_SLICE,
        )? {
            ComponentProgress::Idle => false,
            ComponentProgress::Pending => true,
            ComponentProgress::Finished => return Ok(false),
        };

        if !self.component_work_pending {
            for window in self.windows.iter().filter(|window| window.is_closing()) {
                window.unmount();
            }
            self.windows.retain(|window| !window.is_closing());
        }
        Ok(!self.windows.is_empty())
    }
}

/// Drives all Window groups using their shared current-thread Win32 queue.
pub(crate) struct WindowEventLoop {
    message_pump: WindowsMessagePump,
    groups: Vec<WindowGroup>,
    next_presentation_wait: usize,
}

impl WindowEventLoop {
    pub(crate) fn new(groups: impl IntoIterator<Item = WindowGroup>) -> Self {
        Self {
            message_pump: WindowsMessagePump::new(),
            groups: groups.into_iter().collect(),
            next_presentation_wait: 0,
        }
    }

    pub(crate) fn run(mut self) -> Result<()> {
        while !self.groups.is_empty() {
            let now = Instant::now();
            for group in &mut self.groups {
                group.prepare_animation_frames(now);
            }
            let message_progress = if self.groups.iter().all(WindowGroup::is_idle) {
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
            let mut index = 0;
            while index < self.groups.len() {
                if self.groups[index].process_ready_work(now)? {
                    index += 1;
                } else {
                    self.groups.remove(index);
                }
            }
            if self.groups.is_empty() {
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

            let mut flat_index = 0;
            for group in &mut self.groups {
                for window in &mut group.windows {
                    let wait = if Some(flat_index) == waiting_window {
                        PRESENTATION_WAIT_SLICE
                    } else {
                        Duration::ZERO
                    };
                    window.advance_presentation(wait)?;
                    flat_index += 1;
                }
            }
            self.next_presentation_wait %= flat_index;
        }
        Ok(())
    }

    fn next_window_waiting_for_presentation(&self) -> Option<usize> {
        let count = self
            .groups
            .iter()
            .flat_map(|group| group.windows.iter())
            .count();
        if count == 0 {
            return None;
        }
        let start = self.next_presentation_wait % count;
        let windows = || {
            self.groups
                .iter()
                .flat_map(|group| group.windows.iter())
                .enumerate()
        };
        windows()
            .skip(start)
            .find(|(_, window)| window.has_pending_presentation())
            .or_else(|| {
                windows()
                    .take(start)
                    .find(|(_, window)| window.has_pending_presentation())
            })
            .map(|(index, _)| index)
    }

    fn time_until_animation_frame(&self, now: Instant) -> Option<Duration> {
        self.groups
            .iter()
            .filter_map(|group| group.time_until_animation_frame(now))
            .min()
    }
}
