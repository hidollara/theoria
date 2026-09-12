//! Semantic events accumulated by one native Window procedure.

use std::rc::Rc;

use crate::{
    surface::SurfaceSize,
    window::backend::{WindowBackendEvent, WindowEventBuffer},
};

pub(super) struct WindowState {
    size: SurfaceSize,
    events: Rc<WindowEventBuffer>,
}

impl WindowState {
    pub(super) fn new(size: SurfaceSize, events: Rc<WindowEventBuffer>) -> Self {
        Self { size, events }
    }

    /// Replaces messages emitted during native construction with the first
    /// redraw for the final client extent.
    pub(super) fn finish_initialization(&mut self, size: SurfaceSize) {
        self.size = size;
        self.events.reset_for_initial_redraw();
    }

    pub(super) const fn size(&self) -> SurfaceSize {
        self.size
    }

    pub(super) fn request_redraw(&mut self) {
        self.events.request_redraw();
    }

    pub(super) fn resize(&mut self, size: SurfaceSize) {
        self.size = size;

        self.events.resize(size);
    }

    pub(super) fn push(&mut self, event: WindowBackendEvent) {
        self.events.push(event);
    }
}
