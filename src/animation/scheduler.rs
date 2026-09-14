//! Internal scheduling that connects Window frame opportunities to Components.

use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};

use crate::{
    component::MountContext,
    event::{Channel, Dispatch, EventSink, OwningSink},
    reactive::Value,
};

use super::{AnimationFrame, FrameClock, FrameRequests};

const DEFAULT_FRAME_INTERVAL: Duration = Duration::from_nanos(16_666_667);

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
    channel: Channel<FrameOpportunity>,
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
        let channel = Channel::new();
        let requests = Rc::new(FrameRequests {
            pending: Cell::new(false),
        });
        Self {
            channel,
            requests,
            timing: Rc::new(FrameTiming {
                origin,
                previous: Cell::new(None),
            }),
            deadline: None,
            interval,
        }
    }

    fn clock(&self, frame: Value<AnimationFrame>) -> FrameClock {
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
        assert!(context.connect(self.channel.source(), OwningSink::new(frames)));
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

    pub(crate) const fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    /// Publishes a due frame and consumes its request.
    pub(crate) fn publish_due(&mut self, now: Instant) -> bool {
        if !self.take_due(now) {
            return false;
        }
        debug_assert_eq!(
            self.channel.sink().send(FrameOpportunity).run(),
            crate::event::DispatchOutcome::Completed,
            "the FrameScheduler owns its live Channel and publishes inline"
        );
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
    use crate::{component::ComponentInstance, event::DispatchQueue, reactive::Reactive};

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
        assert_eq!(scheduler.deadline(), Some(origin));
        assert!(scheduler.take_due(origin));
        assert_eq!(
            scheduler.sample(origin),
            AnimationFrame {
                elapsed: Duration::ZERO,
                delta: Duration::ZERO,
            }
        );

        scheduler.prepare(origin, true);
        assert_eq!(scheduler.deadline(), None);
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
        assert_eq!(scheduler.deadline(), Some(origin + INTERVAL));
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
        assert_eq!(scheduler.deadline(), Some(late));
        assert!(scheduler.take_due(late));
        assert_eq!(
            scheduler.sample(late),
            AnimationFrame {
                elapsed: Duration::from_secs(1),
                delta: Duration::from_secs(1),
            }
        );
        assert_eq!(scheduler.deadline(), None);
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
        assert_eq!(scheduler.deadline(), None);

        let restored = origin + Duration::from_secs(1);
        scheduler.prepare(restored, true);
        assert_eq!(scheduler.deadline(), Some(restored));
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
        let mut queue = DispatchQueue::new();
        let first_observer = Rc::new(Cell::new(None));
        let second_observer = Rc::new(Cell::new(None));
        let first_output = Rc::clone(&first_observer);
        let second_output = Rc::clone(&second_observer);
        let mut clock = None;
        let mut components = ComponentInstance::new(queue.dispatcher(), |context| {
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

        assert!(queue.run_next().is_some());
        assert!(scheduler.timing.previous.get().is_some());
        assert!(queue.run_next().is_none());
        components
            .flush_effects()
            .expect("the frame observers are valid Effects");

        assert!(first_observer.get().is_some());
        assert_eq!(first_observer.get(), second_observer.get());
    }
}
