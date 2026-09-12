//! Platform-independent drawable-surface capabilities and content.

use crate::{
    animation::FrameClock,
    canvas::{Canvas, CanvasView},
    color::Color,
    event::EventSource,
    reactive::Value,
};

/// The finite extent of a drawable surface in surface pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SurfaceSize {
    pub width: u32,
    pub height: u32,
}

/// An observation concerning one finite drawable surface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SurfaceEvent {
    Resized(SurfaceSize),
    RedrawRequested,
}

/// Observable capabilities of one finite drawable surface.
///
/// A `Surface` describes the portable drawing boundary exposed by a host. It
/// does not own a native Window or prescribe where its events originate.
#[derive(Clone)]
pub struct Surface {
    events: EventSource<SurfaceEvent>,
    size: Value<SurfaceSize>,
    frame_clock: FrameClock,
}

impl Surface {
    pub(crate) const fn new(
        events: EventSource<SurfaceEvent>,
        size: Value<SurfaceSize>,
        frame_clock: FrameClock,
    ) -> Self {
        Self {
            events,
            size,
            frame_clock,
        }
    }

    /// Observations concerning this drawable surface.
    pub const fn events(&self) -> &EventSource<SurfaceEvent> {
        &self.events
    }

    /// Current drawable extent as a reactive value.
    pub fn size(&self) -> Value<SurfaceSize> {
        self.size.clone()
    }

    /// This surface's request-driven animation frame clock.
    pub fn frame_clock(&self) -> FrameClock {
        self.frame_clock.clone()
    }
}

/// Backend-independent Canvas content to present on a [`Surface`].
///
/// The renderer determines its physical GPU execution after a terminal Effect
/// snapshots it.
#[derive(Clone, Debug)]
pub struct SurfaceContent {
    canvas: Canvas,
    view: CanvasView,
    clear_color: Color,
}

impl SurfaceContent {
    pub const fn new(canvas: Canvas, view: CanvasView, clear_color: Color) -> Self {
        Self {
            canvas,
            view,
            clear_color,
        }
    }

    pub const fn canvas(&self) -> &Canvas {
        &self.canvas
    }

    pub const fn view(&self) -> CanvasView {
        self.view
    }

    pub const fn clear_color(&self) -> Color {
        self.clear_color
    }
}
