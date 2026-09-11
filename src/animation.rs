//! Request-driven animation frame scheduling for one drawable Window.

use std::{
    cell::Cell,
    fmt,
    rc::{Rc, Weak},
    time::{Duration, Instant},
};

use crate::{
    component::MountContext,
    event::{DispatchRoute, EventProvider, EventSource, channel},
    reactive::Value,
};

const DEFAULT_FRAME_INTERVAL: Duration = Duration::from_nanos(16_666_667);

/// Monotonic timing information for one requested animation update.
///
/// This describes when Theoria made an update available. It is not a measured
/// display-presentation timestamp; native presentation remains asynchronous.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AnimationFrame {
    elapsed: Duration,
    delta: Duration,
}

impl AnimationFrame {
    /// Time elapsed since this Window's frame clock was created.
    pub fn elapsed(self) -> Duration {
        self.elapsed
    }

    /// Time elapsed since the previously produced animation frame.
    ///
    /// The first frame has a zero delta.
    pub fn delta(self) -> Duration {
        self.delta
    }
}

impl From<&AnimationFrame> for AnimationFrame {
    fn from(frame: &AnimationFrame) -> Self {
        *frame
    }
}

/// A capability for observing and requesting animation frames.
///
/// Every animation derived from [`frame`](Self::frame) observes the same time
/// for one Window update. Requests are coalesced until the next frame is
/// produced. Producing a frame consumes the current request; continuously
/// animating code requests another frame from an Effect which observes the
/// current frame. With no request, the Window can remain asleep indefinitely.
#[derive(Clone)]
pub struct FrameClock {
    frame: Value<AnimationFrame>,
    requests: Weak<FrameRequests>,
}

impl FrameClock {
    /// Returns this Window's shared current animation frame.
    ///
    /// Before the first requested frame is produced, both timing values are
    /// zero. A produced frame updates this one reactive value, so all dependent
    /// computations flushed for that update observe the same timestamp.
    pub fn frame(&self) -> Value<AnimationFrame> {
        self.frame.clone()
    }

    /// Requests one future animation frame.
    ///
    /// Repeated requests before that frame are coalesced. A request held while
    /// the Window has no drawable extent resumes after it becomes drawable.
    /// Returns `false` when the owning Window has already been dropped.
    #[must_use]
    pub fn request_frame(&self) -> bool {
        let Some(requests) = self.requests.upgrade() else {
            return false;
        };
        requests.pending.set(true);
        true
    }
}

impl fmt::Debug for FrameClock {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("FrameClock").finish_non_exhaustive()
    }
}

struct FrameRequests {
    pending: Cell<bool>,
}

#[derive(Clone, Copy)]
struct FrameOpportunity;

struct FrameTiming {
    origin: Instant,
    previous: Cell<Option<Instant>>,
}

impl FrameTiming {
    fn sample(&self, now: Instant) -> AnimationFrame {
        let previous = self.previous.replace(Some(now));
        AnimationFrame {
            elapsed: now.duration_since(self.origin),
            delta: previous.map_or(Duration::ZERO, |previous| now.duration_since(previous)),
        }
    }
}

pub(crate) struct FrameScheduler {
    provider: EventProvider<FrameOpportunity>,
    opportunities: EventSource<FrameOpportunity>,
    requests: Rc<FrameRequests>,
    timing: Rc<FrameTiming>,
    deadline: Option<Instant>,
    interval: Duration,
}

impl FrameScheduler {
    pub(crate) fn new() -> Self {
        Self::with_timing(Instant::now(), DEFAULT_FRAME_INTERVAL)
    }

    fn with_timing(origin: Instant, interval: Duration) -> Self {
        let (provider, opportunities) = channel();
        let requests = Rc::new(FrameRequests {
            pending: Cell::new(false),
        });
        Self {
            provider,
            opportunities,
            requests,
            timing: Rc::new(FrameTiming {
                origin,
                previous: Cell::new(None),
            }),
            deadline: None,
            interval,
        }
    }

    pub(crate) fn clock(&self, frame: Value<AnimationFrame>) -> FrameClock {
        FrameClock {
            frame,
            requests: Rc::downgrade(&self.requests),
        }
    }

    #[cfg(test)]
    fn sample(&self, now: Instant) -> AnimationFrame {
        self.timing.sample(now)
    }

    pub(crate) fn mount(&self, context: &mut MountContext<'_>) -> FrameClock {
        let frame = context.create_state(AnimationFrame::default());
        let clock = self.clock(frame.value());
        let timing = Rc::clone(&self.timing);
        let frames = context.receive(move |_: FrameOpportunity, context| {
            context.set(&frame, timing.sample(Instant::now()));
        });
        assert!(context.connect_cloned(&self.opportunities, &frames));
        clock
    }

    /// Incorporates requests made since the previous event-loop iteration.
    pub(crate) fn prepare(&mut self, now: Instant, drawable: bool) {
        if !drawable {
            if self.deadline.take().is_some() {
                self.requests.pending.set(true);
            }
            return;
        }
        if !self.requests.pending.replace(false) || self.deadline.is_some() {
            return;
        }
        self.deadline = Some(self.timing.previous.get().map_or(now, |previous| {
            previous
                .checked_add(self.interval)
                .map_or(now, |earliest| earliest.max(now))
        }));
    }

    pub(crate) fn time_until_frame(&self, now: Instant) -> Option<Duration> {
        self.deadline
            .map(|deadline| deadline.saturating_duration_since(now))
    }

    /// Publishes a due frame and consumes its request.
    pub(crate) fn publish_due(&mut self, now: Instant) -> bool {
        if !self.take_due(now) {
            return false;
        }
        let result = DispatchRoute::Inline.send(&self.provider, FrameOpportunity);
        debug_assert!(result.is_ok(), "the FrameScheduler owns its Provider");
        true
    }

    fn take_due(&mut self, now: Instant) -> bool {
        if self.deadline.is_none_or(|deadline| deadline > now) {
            return false;
        }
        self.deadline = None;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{component::ComponentInstance, event::FifoDispatcher, reactive::Reactive};

    const INTERVAL: Duration = Duration::from_millis(10);

    fn clock(scheduler: &FrameScheduler) -> FrameClock {
        let frame = Reactive::new()
            .create_state(AnimationFrame::default())
            .value();
        scheduler.clock(frame)
    }

    #[test]
    fn requests_are_coalesced_until_the_due_frame() {
        let origin = Instant::now();
        let mut scheduler = FrameScheduler::with_timing(origin, INTERVAL);
        let clock = clock(&scheduler);

        assert!(clock.request_frame());
        assert!(clock.request_frame());
        scheduler.prepare(origin, true);
        assert_eq!(scheduler.time_until_frame(origin), Some(Duration::ZERO));
        assert!(scheduler.take_due(origin));
        assert_eq!(
            scheduler.sample(origin),
            AnimationFrame {
                elapsed: Duration::ZERO,
                delta: Duration::ZERO,
            }
        );

        scheduler.prepare(origin, true);
        assert_eq!(scheduler.time_until_frame(origin), None);
    }

    #[test]
    fn a_follow_up_request_observes_the_minimum_frame_interval() {
        let origin = Instant::now();
        let mut scheduler = FrameScheduler::with_timing(origin, INTERVAL);
        let clock = clock(&scheduler);
        assert!(clock.request_frame());
        scheduler.prepare(origin, true);
        assert!(scheduler.take_due(origin));
        scheduler.sample(origin);

        assert!(clock.request_frame());
        scheduler.prepare(origin + Duration::from_millis(2), true);
        assert_eq!(
            scheduler.time_until_frame(origin + Duration::from_millis(2)),
            Some(Duration::from_millis(8))
        );
        assert!(!scheduler.take_due(origin + Duration::from_millis(9)));
        assert!(scheduler.take_due(origin + INTERVAL));
        assert_eq!(
            scheduler.sample(origin + INTERVAL),
            AnimationFrame {
                elapsed: INTERVAL,
                delta: INTERVAL,
            }
        );
    }

    #[test]
    fn a_late_request_does_not_generate_catch_up_frames() {
        let origin = Instant::now();
        let mut scheduler = FrameScheduler::with_timing(origin, INTERVAL);
        let clock = clock(&scheduler);
        assert!(clock.request_frame());
        scheduler.prepare(origin, true);
        assert!(scheduler.take_due(origin));
        scheduler.sample(origin);

        let late = origin + Duration::from_secs(1);
        assert!(clock.request_frame());
        scheduler.prepare(late, true);
        assert_eq!(scheduler.time_until_frame(late), Some(Duration::ZERO));
        assert!(scheduler.take_due(late));
        assert_eq!(
            scheduler.sample(late),
            AnimationFrame {
                elapsed: Duration::from_secs(1),
                delta: Duration::from_secs(1),
            }
        );
        assert_eq!(scheduler.time_until_frame(late), None);
    }

    #[test]
    fn a_clock_does_not_keep_its_scheduler_alive() {
        let clock = {
            let scheduler = FrameScheduler::new();
            clock(&scheduler)
        };

        assert!(!clock.request_frame());
    }

    #[test]
    fn a_request_waits_while_the_window_is_not_drawable() {
        let origin = Instant::now();
        let mut scheduler = FrameScheduler::with_timing(origin, INTERVAL);
        let clock = clock(&scheduler);

        assert!(clock.request_frame());
        scheduler.prepare(origin, false);
        assert_eq!(scheduler.time_until_frame(origin), None);

        let restored = origin + Duration::from_secs(1);
        scheduler.prepare(restored, true);
        assert_eq!(scheduler.time_until_frame(restored), Some(Duration::ZERO));
        assert!(scheduler.take_due(restored));
        assert_eq!(
            scheduler.sample(restored),
            AnimationFrame {
                elapsed: Duration::from_secs(1),
                delta: Duration::ZERO,
            }
        );
    }

    #[test]
    fn frame_time_is_sampled_when_the_opportunity_is_processed() {
        let origin = Instant::now();
        let mut scheduler = FrameScheduler::with_timing(origin, INTERVAL);
        let clock = clock(&scheduler);
        assert!(clock.request_frame());
        scheduler.prepare(origin, true);
        assert!(scheduler.take_due(origin));

        let processed = origin + Duration::from_millis(7);
        assert_eq!(
            scheduler.sample(processed),
            AnimationFrame {
                elapsed: Duration::from_millis(7),
                delta: Duration::ZERO,
            }
        );
    }

    #[test]
    fn a_mounted_scheduler_publishes_one_shared_reactive_frame() {
        let origin = Instant::now();
        let mut scheduler = FrameScheduler::with_timing(origin, INTERVAL);
        let mut dispatcher = FifoDispatcher::new();
        let first_observer = Rc::new(Cell::new(None));
        let second_observer = Rc::new(Cell::new(None));
        let first_output = Rc::clone(&first_observer);
        let second_output = Rc::clone(&second_observer);
        let mut clock = None;
        let mut components = ComponentInstance::new(dispatcher.route(), |context| {
            let mounted_clock = scheduler.mount(context);
            let frame = mounted_clock.frame();
            let first_frame = frame.clone();
            let first = context.create_effect(move |context| {
                first_output.set(Some(*context.read(&first_frame)));
            });
            let second = context.create_effect(move |context| {
                second_output.set(Some(*context.read(&frame)));
            });
            context.sync(&first);
            context.sync(&second);
            clock = Some(mounted_clock);
        });
        let clock = clock.expect("mounting exposes the frame clock");

        assert!(clock.request_frame());
        scheduler.prepare(origin, true);
        assert!(scheduler.publish_due(origin));
        assert!(scheduler.timing.previous.get().is_none());

        assert!(dispatcher.deliver_next());
        assert!(scheduler.timing.previous.get().is_some());
        assert!(!dispatcher.deliver_next());
        components
            .flush_effects()
            .expect("the frame observers are valid Effects");

        assert!(first_observer.get().is_some());
        assert_eq!(first_observer.get(), second_observer.get());
    }
}
