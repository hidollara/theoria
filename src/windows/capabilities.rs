//! Portable projections supplied by the built-in Win32 Window.

use crate::{
    animation::FrameClock,
    event::ChannelSource,
    input::{
        pointer::{MovementThreshold, PointerEvent},
        scroll::ScrollEvent,
    },
    surface::Surface,
    window::CloseRequested,
};

/// Standard Theoria projections exposed by the built-in Win32 Window.
///
/// These capabilities are convenient, not mandatory. A Windows integration
/// may expose additional native event sources, and applications may compose
/// their own projections directly with Theoria Event and Reactive primitives.
pub struct WindowCapabilities {
    surface: Surface,
    pointer: ChannelSource<PointerEvent>,
    scroll: ChannelSource<ScrollEvent>,
    close_requested: ChannelSource<CloseRequested>,
    frame_clock: FrameClock,
    movement_threshold: MovementThreshold,
}

impl WindowCapabilities {
    pub(super) const fn new(
        surface: Surface,
        pointer: ChannelSource<PointerEvent>,
        scroll: ChannelSource<ScrollEvent>,
        close_requested: ChannelSource<CloseRequested>,
        frame_clock: FrameClock,
        movement_threshold: MovementThreshold,
    ) -> Self {
        Self {
            surface,
            pointer,
            scroll,
            close_requested,
            frame_clock,
            movement_threshold,
        }
    }

    pub const fn surface(&self) -> &Surface {
        &self.surface
    }

    pub const fn pointer(&self) -> &ChannelSource<PointerEvent> {
        &self.pointer
    }

    pub const fn scroll(&self) -> &ChannelSource<ScrollEvent> {
        &self.scroll
    }

    pub const fn close_requested(&self) -> &ChannelSource<CloseRequested> {
        &self.close_requested
    }

    /// Returns this Window's request-driven animation frame clock.
    pub fn frame_clock(&self) -> FrameClock {
        self.frame_clock.clone()
    }

    /// Host-provided default movement boundary for click and drag recognition.
    ///
    /// The host resolves this from its platform's input metrics. Applications
    /// may choose another policy per device.
    pub const fn movement_threshold(&self) -> MovementThreshold {
        self.movement_threshold
    }
}
