//! Thread-level coordination of native input and registered local work.

use std::{
    cell::RefCell,
    error::Error,
    fmt, io,
    rc::{Rc, Weak},
    time::{Duration, Instant},
};

use crate::windows::message_pump::{
    MessagePumpEvent, MessagePumpWaker, ThreadMessage, WindowsMessagePump,
};

mod bridge;

pub use self::bridge::{EventLoopBridge, EventLoopBridgeSendError, EventLoopBridgeSender};

/// Time an otherwise idle iteration may lend to one waitable work item.
const WAIT_SLICE: Duration = Duration::from_millis(1);

type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// Drives the current thread's native messages and registered local work.
///
/// An EventLoop owns its platform message pump but does not own registered
/// Windows, Components, or other producers. Those objects retain themselves
/// through their own lifetime boundaries and register here non-owningly.
pub struct EventLoop {
    message_pump: WindowsMessagePump,
    registrations: Rc<Registrations>,
    next_waitable_work: usize,
}

impl EventLoop {
    /// Creates an independent driver for the current GUI thread.
    pub fn new() -> Self {
        Self {
            message_pump: WindowsMessagePump::new(),
            registrations: Rc::new(Registrations::default()),
            next_waitable_work: 0,
        }
    }

    /// Returns a non-owning capability for registering work with this loop.
    pub fn handle(&self) -> EventLoopHandle {
        EventLoopHandle {
            registrations: Rc::downgrade(&self.registrations),
            message_pump: self.message_pump.waker(),
        }
    }

    /// Returns a thread-safe, non-owning capability for interrupting this loop's
    /// native wait.
    ///
    /// Waking does not itself enqueue application work. Producers that cross an
    /// execution boundary should first make their work observable, then wake the
    /// loop so registered local work can consume it.
    pub fn waker(&self) -> EventLoopWaker {
        EventLoopWaker {
            message_pump: self.message_pump.waker(),
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

            let waiting_work = matches!(message_progress, MessagePumpEvent::Idle)
                .then(|| self.next_waitable_work(&participants))
                .flatten();
            if let Some(index) = waiting_work {
                self.next_waitable_work = index + 1;
            }

            let mut flat_index = 0;
            for participant in &participants {
                for work in 0..participant.waitable_work_count() {
                    let wait = if Some(flat_index) == waiting_work {
                        WAIT_SLICE
                    } else {
                        Duration::ZERO
                    };
                    participant.advance_waitable_work(work, wait)?;
                    flat_index += 1;
                }
            }
            if flat_index != 0 {
                self.next_waitable_work %= flat_index;
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

    fn next_waitable_work(&self, participants: &[Rc<dyn EventLoopParticipant>]) -> Option<usize> {
        let count = participants
            .iter()
            .map(|participant| participant.waitable_work_count())
            .sum::<usize>();
        if count == 0 {
            return None;
        }
        let start = self.next_waitable_work % count;
        (start..count)
            .chain(0..start)
            .find(|&index| waitable_work_pending(participants, index))
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
    message_pump: MessagePumpWaker,
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

    /// Creates a cross-thread event Bridge driven by this EventLoop.
    ///
    /// The returned Bridge owns the receiving side and exposes its local Source.
    /// Its Sender makes an event available before waking this loop. Neither
    /// capability keeps the EventLoop alive.
    pub fn bridge<E>(
        &self,
    ) -> std::result::Result<(EventLoopBridge<E>, EventLoopBridgeSender<E>), EventLoopClosed>
    where
        E: Send + 'static,
    {
        bridge::create(self)
    }

    /// Returns a thread-safe, non-owning capability for interrupting this loop.
    pub fn waker(&self) -> EventLoopWaker {
        EventLoopWaker {
            message_pump: self.message_pump.clone(),
        }
    }
}

impl fmt::Debug for EventLoopHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EventLoopHandle")
            .finish_non_exhaustive()
    }
}

/// A cloneable capability for interrupting one [`EventLoop`]'s native wait.
///
/// Unlike [`EventLoopHandle`], a Waker is `Send + Sync` and may cross execution
/// boundaries. It does not retain the EventLoop or any registered work.
#[derive(Clone)]
pub struct EventLoopWaker {
    message_pump: MessagePumpWaker,
}

impl EventLoopWaker {
    /// Interrupts the EventLoop's current or next native wait.
    ///
    /// Returns [`io::ErrorKind::BrokenPipe`] after the EventLoop has been
    /// dropped. Other errors report that Win32 rejected the wake request.
    pub fn wake(&self) -> io::Result<()> {
        self.message_pump.wake()
    }
}

impl fmt::Debug for EventLoopWaker {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EventLoopWaker")
            .finish_non_exhaustive()
    }
}

#[derive(Default)]
struct Registrations {
    participants: RefCell<Vec<Weak<dyn EventLoopParticipant>>>,
}

/// Registration failed because its EventLoop has already been dropped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EventLoopClosed;

impl fmt::Display for EventLoopClosed {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("the EventLoop is no longer available")
    }
}

impl Error for EventLoopClosed {}

pub(crate) trait EventLoopParticipant {
    fn is_finished(&self) -> bool;
    fn prepare(&self, _now: Instant) {}
    fn next_deadline(&self) -> Option<Instant> {
        None
    }
    fn is_idle(&self) -> bool {
        true
    }
    fn process_ready_work(&self, now: Instant) -> Result<()>;
    fn waitable_work_count(&self) -> usize {
        0
    }
    fn waitable_work_pending(&self, _index: usize) -> bool {
        false
    }
    fn advance_waitable_work(&self, _index: usize, _wait: Duration) -> Result<()> {
        Ok(())
    }
}

fn next_deadline(participants: &[Rc<dyn EventLoopParticipant>]) -> Option<Instant> {
    participants
        .iter()
        .filter_map(|participant| participant.next_deadline())
        .min()
}

fn waitable_work_pending(
    participants: &[Rc<dyn EventLoopParticipant>],
    mut flat_index: usize,
) -> bool {
    for participant in participants {
        let count = participant.waitable_work_count();
        if flat_index < count {
            return participant.waitable_work_pending(flat_index);
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

        fn process_ready_work(&self, _: Instant) -> Result<()> {
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

    #[test]
    fn a_waker_is_safe_to_move_between_threads() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<EventLoopWaker>();
    }

    #[test]
    fn a_waker_does_not_keep_its_event_loop_available() {
        let event_loop = EventLoop::new();
        let waker = event_loop.waker();
        drop(event_loop);

        assert_eq!(waker.wake().unwrap_err().kind(), io::ErrorKind::BrokenPipe);
    }
}
