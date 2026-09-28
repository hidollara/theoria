//! Capabilities supplied by the built-in Win32 Window.

use crate::{
    animation::FrameClock,
    event::ChannelSource,
    input::{
        pointer::{MovementThreshold, PointerEvent},
        scroll::ScrollEvent,
    },
    surface::Surface,
    window::CloseRequested,
    windows::native_window::Win32WindowMessageSource,
};

/// Portable projections and native observations exposed by a Win32 Window.
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
    native_messages: Win32WindowMessageSource,
}

impl WindowCapabilities {
    pub(super) const fn new(
        surface: Surface,
        pointer: ChannelSource<PointerEvent>,
        scroll: ChannelSource<ScrollEvent>,
        close_requested: ChannelSource<CloseRequested>,
        frame_clock: FrameClock,
        movement_threshold: MovementThreshold,
        native_messages: Win32WindowMessageSource,
    ) -> Self {
        Self {
            surface,
            pointer,
            scroll,
            close_requested,
            frame_clock,
            movement_threshold,
            native_messages,
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

    /// Returns raw messages observed by this Window's Win32 procedure.
    ///
    /// Message parameters may borrow native storage that is valid only while
    /// the procedure is running, so this Source must be connected inline.
    pub const fn native_messages(&self) -> &Win32WindowMessageSource {
        &self.native_messages
    }
}
