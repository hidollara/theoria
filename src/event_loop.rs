//! Thread-level coordination of native input and registered local work.

use std::{
    cell::RefCell,
    error::Error,
    fmt,
    rc::{Rc, Weak},
    time::{Duration, Instant},
};

use crate::windows::message_pump::{MessagePumpEvent, ThreadMessage, WindowsMessagePump};

/// Time an otherwise idle iteration may lend to presentation progress.
const PRESENTATION_WAIT_SLICE: Duration = Duration::from_millis(1);

type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// Drives the current thread's native messages and registered local work.
///
/// An EventLoop owns its platform message pump but does not own registered
/// Windows, Components, or other producers. Those objects retain themselves
/// through their own lifetime boundaries and register here non-owningly.
pub struct EventLoop {
    message_pump: WindowsMessagePump,
    registrations: Rc<Registrations>,
    next_presentation_wait: usize,
}

impl EventLoop {
    /// Creates an independent driver for the current GUI thread.
    pub fn new() -> Self {
        Self {
            message_pump: WindowsMessagePump::new(),
            registrations: Rc::new(Registrations::default()),
            next_presentation_wait: 0,
        }
    }

    /// Returns a non-owning capability for registering work with this loop.
    pub fn handle(&self) -> EventLoopHandle {
        EventLoopHandle {
            registrations: Rc::downgrade(&self.registrations),
        }
    }

    /// Drives native messages and registered work until no registration remains
    /// active or the current thread receives its quit message.
    pub fn run(&mut self) -> Result<()> {
        loop {
            let participants = self.live_participants();
            if participants.is_empty() {
                return Ok(());
            }

            let now = Instant::now();
            for participant in &participants {
                participant.prepare(now);
            }
            let message_progress = if participants.iter().all(|participant| participant.is_idle()) {
                if let Some(deadline) = next_deadline(&participants) {
                    self.message_pump
                        .wait_for(deadline.saturating_duration_since(now))?
                } else {
                    self.message_pump.wait()?
                }
            } else {
                self.message_pump.poll()
            };
            match message_progress {
                MessagePumpEvent::Quit(_exit_code) => return Ok(()),
                MessagePumpEvent::ThreadMessage(ThreadMessage {
                    message: _message,
                    wparam: _wparam,
                    lparam: _lparam,
                }) => {
                    // No registered thread-message producer consumes this yet.
                }
                MessagePumpEvent::WindowDispatched | MessagePumpEvent::Idle => {}
            }

            let now = Instant::now();
            for participant in &participants {
                participant.process_ready_work(now)?;
            }

            let participants = self.live_participants();
            if participants.is_empty() {
                return Ok(());
            }

            let waiting_presentation = matches!(message_progress, MessagePumpEvent::Idle)
                .then(|| self.next_presentation_waiting(&participants))
                .flatten();
            if let Some(index) = waiting_presentation {
                self.next_presentation_wait = index + 1;
            }

            let mut flat_index = 0;
            for participant in &participants {
                for presentation in 0..participant.presentation_count() {
                    let wait = if Some(flat_index) == waiting_presentation {
                        PRESENTATION_WAIT_SLICE
                    } else {
                        Duration::ZERO
                    };
                    participant.advance_presentation(presentation, wait)?;
                    flat_index += 1;
                }
            }
            if flat_index != 0 {
                self.next_presentation_wait %= flat_index;
            }
        }
    }

    fn live_participants(&self) -> Vec<Rc<dyn EventLoopParticipant>> {
        let participants: Vec<_> = self
            .registrations
            .participants
            .borrow()
            .iter()
            .filter_map(Weak::upgrade)
            .collect();
        self.registrations
            .participants
            .borrow_mut()
            .retain(|participant| participant.strong_count() != 0);
        participants
            .into_iter()
            .filter(|participant| !participant.is_finished())
            .collect()
    }

    fn next_presentation_waiting(
        &self,
        participants: &[Rc<dyn EventLoopParticipant>],
    ) -> Option<usize> {
        let count = participants
            .iter()
            .map(|participant| participant.presentation_count())
            .sum::<usize>();
        if count == 0 {
            return None;
        }
        let start = self.next_presentation_wait % count;
        (start..count)
            .chain(0..start)
            .find(|&index| presentation_pending(participants, index))
    }
}

impl Default for EventLoop {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for EventLoop {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("EventLoop").finish_non_exhaustive()
    }
}

/// A cloneable, non-owning registration capability for one [`EventLoop`].
///
/// Keeping a handle alive does not keep the EventLoop or any registered work
/// alive. It may be retained by Components that create driven resources later.
#[derive(Clone)]
pub struct EventLoopHandle {
    registrations: Weak<Registrations>,
}

impl EventLoopHandle {
    pub(crate) fn register<T>(
        &self,
        participant: &Rc<T>,
    ) -> std::result::Result<(), EventLoopClosed>
    where
        T: EventLoopParticipant + 'static,
    {
        let Some(registrations) = self.registrations.upgrade() else {
            return Err(EventLoopClosed);
        };
        let participant: Rc<dyn EventLoopParticipant> = participant.clone();
        registrations
            .participants
            .borrow_mut()
            .push(Rc::downgrade(&participant));
        Ok(())
    }
}

impl fmt::Debug for EventLoopHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EventLoopHandle")
            .finish_non_exhaustive()
    }
}

#[derive(Default)]
struct Registrations {
    participants: RefCell<Vec<Weak<dyn EventLoopParticipant>>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EventLoopClosed;

impl fmt::Display for EventLoopClosed {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("the EventLoop is no longer available")
    }
}

impl Error for EventLoopClosed {}

pub(crate) trait EventLoopParticipant {
    fn is_finished(&self) -> bool;
    fn prepare(&self, now: Instant);
    fn next_deadline(&self) -> Option<Instant>;
    fn is_idle(&self) -> bool;
    fn process_ready_work(&self, now: Instant) -> Result<()>;
    fn presentation_count(&self) -> usize;
    fn presentation_pending(&self, index: usize) -> bool;
    fn advance_presentation(&self, index: usize, wait: Duration) -> Result<()>;
}

fn next_deadline(participants: &[Rc<dyn EventLoopParticipant>]) -> Option<Instant> {
    participants
        .iter()
        .filter_map(|participant| participant.next_deadline())
        .min()
}

fn presentation_pending(
    participants: &[Rc<dyn EventLoopParticipant>],
    mut flat_index: usize,
) -> bool {
    for participant in participants {
        let count = participant.presentation_count();
        if flat_index < count {
            return participant.presentation_pending(flat_index);
        }
        flat_index -= count;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FinishedParticipant;

    impl EventLoopParticipant for FinishedParticipant {
        fn is_finished(&self) -> bool {
            true
        }

        fn prepare(&self, _: Instant) {}

        fn next_deadline(&self) -> Option<Instant> {
            None
        }

        fn is_idle(&self) -> bool {
            true
        }

        fn process_ready_work(&self, _: Instant) -> Result<()> {
            Ok(())
        }

        fn presentation_count(&self) -> usize {
            0
        }

        fn presentation_pending(&self, _: usize) -> bool {
            false
        }

        fn advance_presentation(&self, _: usize, _: Duration) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn an_empty_event_loop_finishes_without_waiting_for_messages() {
        EventLoop::new()
            .run()
            .expect("an empty event loop finishes immediately");
    }

    #[test]
    fn registration_does_not_keep_a_participant_alive() {
        let event_loop = EventLoop::new();
        let handle = event_loop.handle();
        let participant = Rc::new(FinishedParticipant);
        let weak = Rc::downgrade(&participant);

        handle.register(&participant).unwrap();
        drop(participant);

        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn a_handle_does_not_keep_its_event_loop_alive() {
        let event_loop = EventLoop::new();
        let handle = event_loop.handle();
        drop(event_loop);

        assert_eq!(
            handle.register(&Rc::new(FinishedParticipant)),
            Err(EventLoopClosed)
        );
    }
}
