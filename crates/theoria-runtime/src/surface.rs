//! Platform-independent drawable-surface capabilities and content.

use crate::{event::ChannelSource, reactive::Value};

pub use theoria_renderer::{OutputAlpha, OutputImage, SurfaceContent, SurfaceSize, Viewport};

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
