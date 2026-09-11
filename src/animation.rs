//! Request-driven animation frame scheduling for one drawable Window.

use std::{cell::Cell, fmt, rc::Weak, time::Duration};

use crate::reactive::Value;

mod scheduler;

pub(crate) use scheduler::FrameScheduler;

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
