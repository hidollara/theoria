use crate::geometry::{Offset, Point};

/// Distance reported by a scrolling input source.
///
/// A step is one conventional wheel detent. Fractional steps preserve
/// high-resolution wheel input. Pixels are available for platforms that
/// report a direct surface-space displacement.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScrollDelta {
    Steps(Offset),
    Pixels(Offset),
}

/// A scroll observation located in its current input coordinate space.
///
/// Positive x points right and positive y points down. The delta describes
/// requested scroll direction, not the distance that content must move;
/// scroll behavior decides that later.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollEvent {
    pub position: Point,
    pub delta: ScrollDelta,
}
