//! State retained behind one native Win32 Window procedure.

use crate::{surface::SurfaceSize, windows::window_message::WindowMessages};

pub(super) struct WindowState {
    size: SurfaceSize,
    messages: WindowMessages,
}

impl WindowState {
    pub(super) fn new(size: SurfaceSize, messages: WindowMessages) -> Self {
        Self { size, messages }
    }

    /// Records the final client extent after native construction.
    pub(super) fn finish_initialization(&mut self, size: SurfaceSize) {
        self.size = size;
    }

    pub(super) const fn size(&self) -> SurfaceSize {
        self.size
    }

    pub(super) const fn messages(&self) -> &WindowMessages {
        &self.messages
    }

    pub(super) fn resize(&mut self, size: SurfaceSize) {
        self.size = size;
    }
}
