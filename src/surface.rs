//! Platform-independent drawable-surface capabilities and content.

use crate::{
    canvas::{Canvas, CanvasView, Color},
    event::ChannelSource,
    reactive::Value,
};

pub use theoria_renderer::{OutputAlpha, OutputImage, SurfaceSize, Viewport};

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
    events: ChannelSource<SurfaceEvent>,
    size: Value<SurfaceSize>,
}

impl Surface {
    /// Combines one producer's observable Surface event and size capabilities.
    pub const fn new(events: ChannelSource<SurfaceEvent>, size: Value<SurfaceSize>) -> Self {
        Self { events, size }
    }

    /// Observations concerning this drawable surface.
    pub const fn events(&self) -> &ChannelSource<SurfaceEvent> {
        &self.events
    }

    /// Current drawable extent as a reactive value.
    pub fn size(&self) -> Value<SurfaceSize> {
        self.size.clone()
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
    pub fn new(canvas: Canvas, view: CanvasView, clear_color: impl Into<Color>) -> Self {
        Self {
            canvas,
            view,
            clear_color: clear_color.into(),
        }
    }

    pub const fn canvas(&self) -> &Canvas {
        &self.canvas
    }

    pub const fn view(&self) -> CanvasView {
        self.view
    }

    pub fn clear_color(&self) -> Color {
        self.clear_color.clone()
    }
}
