//! Thread-level coordination of native input and registered local work.

use std::{
    cell::RefCell,
    error::Error,
    fmt, io,
    rc::{Rc, Weak},
    sync::Weak as SyncWeak,
    time::{Duration, Instant},
};

use crate::event_pump::{EventPump, EventPumpWake, PumpEvent, ThreadMessage};

mod bridge;
mod component;

pub use self::bridge::{EventLoopBridge, EventLoopBridgeSendError, EventLoopBridgeSender};
pub(crate) use self::component::ComponentParticipant;

/// Time an otherwise idle iteration may lend to one waitable work item.
const WAIT_SLICE: Duration = Duration::from_millis(1);
/// Time one iteration lends to each participant's immediately ready work.
const READY_WORK_SLICE: Duration = Duration::from_millis(4);

type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// Coordinates one platform pump and its registered local work.
///
/// An EventLoop owns the platform pump selected when it is constructed, but
/// does not own registered Windows, Components, or other producers. Platform
/// modules construct this value; for example, [`crate::windows::event_loop`]
/// selects the Win32 message pump. Registered objects retain themselves through
/// their own lifetime boundaries and register here non-owningly.
pub struct EventLoop {
    pump: Box<dyn EventPump>,
    registrations: Rc<Registrations>,
    next_waitable_participant: usize,
}

impl EventLoop {
    pub(crate) fn with_pump(pump: Box<dyn EventPump>) -> Self {
        Self {
            pump,
            registrations: Rc::new(Registrations::default()),
            next_waitable_participant: 0,
        }
    }

    /// Returns a non-owning capability for registering work with this loop.
    pub fn handle(&self) -> EventLoopHandle {
        EventLoopHandle {
            registrations: Rc::downgrade(&self.registrations),
            waker: self.waker(),
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
            wake: self.pump.wake_capability(),
        }
    }

    /// Drives native messages and registered work until no participant can
    /// produce or process further work, or the current thread requests exit.
    pub fn run(&mut self) -> Result<()> {
        loop {
            match self.step()? {
                EventLoopStatus::Active => {}
                EventLoopStatus::Empty | EventLoopStatus::ExitRequested(_) => return Ok(()),
            }
        }
    }

    /// Performs one driving step, waiting for native input or the next deadline
    /// when every registered participant is idle.
    ///
    /// This is the composable form of [`run`](Self::run). Callers that own a
    /// surrounding loop can inspect the returned status between steps.
    pub fn step(&mut self) -> Result<EventLoopStatus> {
        self.advance(IdleWait::Allowed)
    }

    /// Performs one driving step without intentionally waiting for work.
    ///
    /// This lets another loop retain responsibility for blocking and call into
    /// Theoria only when appropriate.
    pub fn poll(&mut self) -> Result<EventLoopStatus> {
        self.advance(IdleWait::Forbidden)
    }

    fn advance(&mut self, idle_wait: IdleWait) -> Result<EventLoopStatus> {
        let participants = self.available_participants();
        if should_finish(&participants) {
            return Ok(EventLoopStatus::Empty);
        }

        let now = Instant::now();
        for participant in &participants {
            participant.prepare(now);
        }
        let message_progress = if idle_wait == IdleWait::Allowed
            && participants.iter().all(|participant| participant.is_idle())
        {
            if let Some(deadline) = next_deadline(&participants) {
                self.pump
                    .wait_for(deadline.saturating_duration_since(now))?
            } else {
                self.pump.wait()?
            }
        } else {
            self.pump.poll()?
        };
        match message_progress {
            PumpEvent::ExitRequested(exit_code) => {
                return Ok(EventLoopStatus::ExitRequested(exit_code));
            }
            PumpEvent::ThreadMessage(ThreadMessage {
                message: _message,
                wparam: _wparam,
                lparam: _lparam,
            }) => {
                // No registered thread-message producer consumes this yet.
            }
            PumpEvent::NativeDispatched | PumpEvent::Idle => {}
        }

        let now = Instant::now();
        for participant in &participants {
            participant.process_ready_work(now, READY_WORK_SLICE)?;
        }

        let participants = self.available_participants();
        if should_finish(&participants) {
            return Ok(EventLoopStatus::Empty);
        }

        let waiting_work =
            if idle_wait == IdleWait::Allowed && matches!(message_progress, PumpEvent::Idle) {
                self.next_waitable_participant(&participants)
            } else {
                None
            };
        if let Some(index) = waiting_work {
            self.next_waitable_participant = index + 1;
        }

        for (index, participant) in participants.iter().enumerate() {
            let wait = if Some(index) == waiting_work {
                WAIT_SLICE
            } else {
                Duration::ZERO
            };
            participant.advance_waitable_work(wait)?;
        }
        if !participants.is_empty() {
            self.next_waitable_participant %= participants.len();
        }
        Ok(EventLoopStatus::Active)
    }

    fn available_participants(&self) -> Vec<Rc<dyn EventLoopParticipant>> {
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
            .filter(|participant| !participant.is_closed())
            .collect()
    }

    fn next_waitable_participant(
        &self,
        participants: &[Rc<dyn EventLoopParticipant>],
    ) -> Option<usize> {
        if participants.is_empty() {
            return None;
        }
        let start = self.next_waitable_participant % participants.len();
        (start..participants.len())
            .chain(0..start)
            .find(|&index| participants[index].has_waitable_work())
    }
}

/// State of an [`EventLoop`] after one driving step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventLoopStatus {
    /// At least one registered participant can still make progress.
    Active,
    /// No participant can produce or process further work.
    Empty,
    /// The native thread requested termination with the supplied exit code.
    ExitRequested(i32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IdleWait {
    Allowed,
    Forbidden,
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
    waker: EventLoopWaker,
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
        self.waker.clone()
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
    wake: SyncWeak<dyn EventPumpWake>,
}

impl EventLoopWaker {
    /// Interrupts the EventLoop's current or next native wait.
    ///
    /// Returns [`io::ErrorKind::BrokenPipe`] after the EventLoop has been
    /// dropped. Other errors report that the platform rejected the wake request.
    pub fn wake(&self) -> io::Result<()> {
        self.wake
            .upgrade()
            .ok_or_else(unavailable_wake_error)?
            .wake()
    }
}

fn unavailable_wake_error() -> io::Error {
    io::Error::new(io::ErrorKind::BrokenPipe, "the event loop is unavailable")
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
    /// Returns whether this participant can never process or produce work again.
    fn is_closed(&self) -> bool;

    /// Returns whether this participant can produce future work while idle.
    ///
    /// Passive processors return `false`: pending work is still drained, but an
    /// idle processor alone does not keep the EventLoop running.
    fn keeps_alive(&self) -> bool;

    fn prepare(&self, _now: Instant) {}
    fn next_deadline(&self) -> Option<Instant> {
        None
    }
    /// Returns whether this participant currently has no immediate work.
    ///
    /// A participant that can become ready only through future native input or
    /// a deadline must also return `true` from [`Self::keeps_alive`].
    fn is_idle(&self) -> bool {
        true
    }
    /// Processes immediately ready work within the loop's suggested time slice.
    ///
    /// One indivisible operation may exceed `available_time`; the participant
    /// should yield before starting another operation once the slice is spent.
    fn process_ready_work(&self, now: Instant, available_time: Duration) -> Result<()>;
    /// Returns whether advancing internal work may benefit from a short wait.
    fn has_waitable_work(&self) -> bool {
        false
    }
    /// Advances this participant's internal waitable work.
    ///
    /// The participant owns selection and fairness among its internal work
    /// items. `wait` is nonzero for at most one participant per loop step.
    fn advance_waitable_work(&self, _wait: Duration) -> Result<()> {
        Ok(())
    }
}

fn should_finish(participants: &[Rc<dyn EventLoopParticipant>]) -> bool {
    participants.is_empty()
        || (participants
            .iter()
            .all(|participant| !participant.keeps_alive())
            && participants.iter().all(|participant| participant.is_idle()))
}

fn next_deadline(participants: &[Rc<dyn EventLoopParticipant>]) -> Option<Instant> {
    participants
        .iter()
        .filter_map(|participant| participant.next_deadline())
        .min()
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, sync::Arc};

    use super::*;

    struct TestPump {
        wake: Arc<TestWake>,
    }

    impl TestPump {
        fn new() -> Self {
            Self {
                wake: Arc::new(TestWake),
            }
        }
    }

    impl EventPump for TestPump {
        fn wake_capability(&self) -> SyncWeak<dyn EventPumpWake> {
            let wake: Arc<dyn EventPumpWake> = self.wake.clone();
            Arc::downgrade(&wake)
        }

        fn wait(&mut self) -> crate::event_pump::PumpResult<PumpEvent> {
            Ok(PumpEvent::Idle)
        }

        fn wait_for(&mut self, _: Duration) -> crate::event_pump::PumpResult<PumpEvent> {
            Ok(PumpEvent::Idle)
        }

        fn poll(&mut self) -> crate::event_pump::PumpResult<PumpEvent> {
            Ok(PumpEvent::Idle)
        }
    }

    struct TestWake;

    impl EventPumpWake for TestWake {
        fn wake(&self) -> io::Result<()> {
            Ok(())
        }
    }

    fn test_event_loop() -> EventLoop {
        EventLoop::with_pump(Box::new(TestPump::new()))
    }

    struct FinishedParticipant;

    impl EventLoopParticipant for FinishedParticipant {
        fn is_closed(&self) -> bool {
            true
        }

        fn keeps_alive(&self) -> bool {
            false
        }

        fn process_ready_work(&self, _: Instant, _: Duration) -> Result<()> {
            Ok(())
        }
    }

    struct OneStepParticipant {
        processed: Cell<bool>,
        available_time: Cell<Duration>,
    }

    impl EventLoopParticipant for OneStepParticipant {
        fn is_closed(&self) -> bool {
            self.processed.get()
        }

        fn keeps_alive(&self) -> bool {
            false
        }

        fn is_idle(&self) -> bool {
            false
        }

        fn process_ready_work(&self, _: Instant, available_time: Duration) -> Result<()> {
            self.available_time.set(available_time);
            self.processed.set(true);
            Ok(())
        }
    }

    #[test]
    fn an_empty_event_loop_finishes_without_waiting_for_messages() {
        assert_eq!(
            test_event_loop()
                .step()
                .expect("an empty event loop finishes immediately"),
            EventLoopStatus::Empty
        );
    }

    #[test]
    fn polling_an_empty_event_loop_finishes_without_waiting_for_messages() {
        assert_eq!(
            test_event_loop()
                .poll()
                .expect("an empty event loop finishes immediately"),
            EventLoopStatus::Empty
        );
    }

    #[test]
    fn polling_advances_registered_work_once_without_entering_the_run_loop() {
        let mut event_loop = test_event_loop();
        let participant = Rc::new(OneStepParticipant {
            processed: Cell::new(false),
            available_time: Cell::new(Duration::ZERO),
        });
        event_loop.handle().register(&participant).unwrap();

        assert_eq!(event_loop.poll().unwrap(), EventLoopStatus::Empty);
        assert!(participant.processed.get());
        assert_eq!(participant.available_time.get(), READY_WORK_SLICE);
    }

    struct DormantParticipant;

    impl EventLoopParticipant for DormantParticipant {
        fn is_closed(&self) -> bool {
            false
        }

        fn keeps_alive(&self) -> bool {
            false
        }

        fn process_ready_work(&self, _: Instant, _: Duration) -> Result<()> {
            panic!("a dormant passive participant has no work to process")
        }
    }

    #[test]
    fn a_dormant_passive_participant_does_not_keep_the_loop_alive() {
        let mut event_loop = test_event_loop();
        let participant = Rc::new(DormantParticipant);
        event_loop.handle().register(&participant).unwrap();

        assert_eq!(event_loop.poll().unwrap(), EventLoopStatus::Empty);
    }

    struct PassiveWork {
        pending: Rc<Cell<bool>>,
        processed: Rc<Cell<bool>>,
    }

    impl EventLoopParticipant for PassiveWork {
        fn is_closed(&self) -> bool {
            false
        }

        fn keeps_alive(&self) -> bool {
            false
        }

        fn is_idle(&self) -> bool {
            !self.pending.get()
        }

        fn process_ready_work(&self, _: Instant, _: Duration) -> Result<()> {
            if self.pending.replace(false) {
                self.processed.set(true);
            }
            Ok(())
        }
    }

    struct OneShotProducer {
        pending: Rc<Cell<bool>>,
        finished: Cell<bool>,
    }

    impl EventLoopParticipant for OneShotProducer {
        fn is_closed(&self) -> bool {
            self.finished.get()
        }

        fn keeps_alive(&self) -> bool {
            true
        }

        fn is_idle(&self) -> bool {
            self.finished.get()
        }

        fn process_ready_work(&self, _: Instant, _: Duration) -> Result<()> {
            self.pending.set(true);
            self.finished.set(true);
            Ok(())
        }
    }

    #[test]
    fn passive_work_outlives_the_producer_that_submitted_it() {
        let mut event_loop = test_event_loop();
        let pending = Rc::new(Cell::new(false));
        let processed = Rc::new(Cell::new(false));
        let passive = Rc::new(PassiveWork {
            pending: Rc::clone(&pending),
            processed: Rc::clone(&processed),
        });
        let producer = Rc::new(OneShotProducer {
            pending,
            finished: Cell::new(false),
        });
        let handle = event_loop.handle();
        handle.register(&passive).unwrap();
        handle.register(&producer).unwrap();

        assert_eq!(event_loop.poll().unwrap(), EventLoopStatus::Active);
        assert!(!processed.get());

        assert_eq!(event_loop.poll().unwrap(), EventLoopStatus::Empty);
        assert!(processed.get());
    }

    struct WaitableParticipant {
        waited: Cell<bool>,
    }

    impl EventLoopParticipant for WaitableParticipant {
        fn is_closed(&self) -> bool {
            false
        }

        fn keeps_alive(&self) -> bool {
            true
        }

        fn is_idle(&self) -> bool {
            false
        }

        fn process_ready_work(&self, _: Instant, _: Duration) -> Result<()> {
            Ok(())
        }

        fn has_waitable_work(&self) -> bool {
            true
        }

        fn advance_waitable_work(&self, wait: Duration) -> Result<()> {
            self.waited.set(!wait.is_zero());
            Ok(())
        }
    }

    #[test]
    fn wait_time_rotates_without_exposing_a_participants_internal_work_items() {
        let mut event_loop = test_event_loop();
        let first = Rc::new(WaitableParticipant {
            waited: Cell::new(false),
        });
        let second = Rc::new(WaitableParticipant {
            waited: Cell::new(false),
        });
        let handle = event_loop.handle();
        handle.register(&first).unwrap();
        handle.register(&second).unwrap();

        assert_eq!(event_loop.step().unwrap(), EventLoopStatus::Active);
        assert!(first.waited.get());
        assert!(!second.waited.get());

        assert_eq!(event_loop.step().unwrap(), EventLoopStatus::Active);
        assert!(!first.waited.get());
        assert!(second.waited.get());
    }

    #[test]
    fn registration_does_not_keep_a_participant_alive() {
        let event_loop = test_event_loop();
        let handle = event_loop.handle();
        let participant = Rc::new(FinishedParticipant);
        let weak = Rc::downgrade(&participant);

        handle.register(&participant).unwrap();
        drop(participant);

        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn a_handle_does_not_keep_its_event_loop_alive() {
        let event_loop = test_event_loop();
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
        let event_loop = test_event_loop();
        let waker = event_loop.waker();
        drop(event_loop);

        assert_eq!(waker.wake().unwrap_err().kind(), io::ErrorKind::BrokenPipe);
    }
}
