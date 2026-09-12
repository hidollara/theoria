//! Thread-level driving of native messages, Component work, and presentation.

use std::time::{Duration, Instant};

use crate::windows::message_pump::{MessagePumpProgress, WindowsMessagePump};

use super::{Result, mounted_group::MountedWindowGroup};

/// Time an otherwise idle iteration may lend to GPU presentation progress.
const PRESENTATION_WAIT_SLICE: Duration = Duration::from_millis(1);

/// Drives all Window groups using their shared current-thread Win32 queue.
pub(super) struct WindowEventLoop {
    message_pump: WindowsMessagePump,
    groups: Vec<MountedWindowGroup>,
    next_presentation_wait: usize,
}

impl WindowEventLoop {
    pub(super) fn new(groups: impl IntoIterator<Item = MountedWindowGroup>) -> Self {
        let mut groups: Vec<_> = groups.into_iter().collect();
        groups.retain(|group| !group.is_finished());
        for group in &mut groups {
            group.request_initial_redraws();
        }
        Self {
            message_pump: WindowsMessagePump::new(),
            groups,
            next_presentation_wait: 0,
        }
    }

    pub(super) fn run(mut self) -> Result<()> {
        while !self.groups.is_empty() {
            let now = Instant::now();
            for group in &mut self.groups {
                group.prepare_scheduled_work(now);
            }
            let message_progress = if self.groups.iter().all(MountedWindowGroup::is_idle) {
                if let Some(deadline) = self.next_deadline() {
                    self.message_pump
                        .wait_for(deadline.saturating_duration_since(now))?
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
                for window in group.windows() {
                    let wait = if Some(flat_index) == waiting_window {
                        PRESENTATION_WAIT_SLICE
                    } else {
                        Duration::ZERO
                    };
                    window.borrow_mut().advance_presentation(wait)?;
                    flat_index += 1;
                }
            }
            if flat_index != 0 {
                self.next_presentation_wait %= flat_index;
            }
        }
        Ok(())
    }

    fn next_window_waiting_for_presentation(&self) -> Option<usize> {
        let count = self
            .groups
            .iter()
            .flat_map(MountedWindowGroup::windows)
            .count();
        if count == 0 {
            return None;
        }
        let start = self.next_presentation_wait % count;
        let windows = || {
            self.groups
                .iter()
                .flat_map(MountedWindowGroup::windows)
                .enumerate()
        };
        windows()
            .skip(start)
            .find(|(_, window)| window.borrow().has_pending_presentation())
            .or_else(|| {
                windows()
                    .take(start)
                    .find(|(_, window)| window.borrow().has_pending_presentation())
            })
            .map(|(index, _)| index)
    }

    fn next_deadline(&self) -> Option<Instant> {
        self.groups
            .iter()
            .filter_map(MountedWindowGroup::next_deadline)
            .min()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_group_without_windows_finishes_without_waiting_for_messages() {
        let group =
            MountedWindowGroup::mount(|_, _| Ok(())).expect("empty group construction succeeds");

        WindowEventLoop::new([group])
            .run()
            .expect("an empty event loop finishes immediately");
    }
}
