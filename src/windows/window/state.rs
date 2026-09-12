//! Semantic events accumulated by one native Window procedure.

use std::collections::VecDeque;

use crate::{
    surface::{SurfaceEvent, SurfaceSize},
    window_backend::WindowBackendEvent,
};

pub(super) struct WindowState {
    size: SurfaceSize,
    events: VecDeque<WindowBackendEvent>,
}

impl WindowState {
    pub(super) fn new(size: SurfaceSize) -> Self {
        Self {
            size,
            events: VecDeque::new(),
        }
    }

    /// Replaces messages emitted during native construction with the first
    /// redraw for the final client extent.
    pub(super) fn finish_initialization(&mut self, size: SurfaceSize) {
        self.size = size;
        self.events.clear();
        self.request_redraw();
    }

    pub(super) const fn size(&self) -> SurfaceSize {
        self.size
    }

    pub(super) fn request_redraw(&mut self) {
        let redraw = WindowBackendEvent::Surface(SurfaceEvent::RedrawRequested);
        if !self.events.contains(&redraw) {
            self.events.push_back(redraw);
        }
    }

    pub(super) fn resize(&mut self, size: SurfaceSize) {
        self.size = size;

        // A renderer only needs the newest surface size. Keep the resize ahead
        // of its redraw so stale intermediate window sizes cannot build up
        // while an expensive frame is being prepared.
        self.events
            .retain(|event| *event != WindowBackendEvent::Surface(SurfaceEvent::RedrawRequested));
        if let Some(event) = self
            .events
            .iter_mut()
            .find(|event| matches!(event, WindowBackendEvent::Surface(SurfaceEvent::Resized(_))))
        {
            *event = WindowBackendEvent::Surface(SurfaceEvent::Resized(size));
        } else {
            self.events
                .push_back(WindowBackendEvent::Surface(SurfaceEvent::Resized(size)));
        }
        self.request_redraw();
    }

    pub(super) fn push(&mut self, event: WindowBackendEvent) {
        self.events.push_back(event);
    }

    pub(super) fn has_pending_event(&self) -> bool {
        !self.events.is_empty()
    }

    pub(super) fn pop_event(&mut self) -> Option<WindowBackendEvent> {
        self.events.pop_front()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redraw_requests_are_coalesced() {
        let mut state = WindowState::new(SurfaceSize {
            width: 100,
            height: 100,
        });

        state.request_redraw();
        state.request_redraw();

        assert_eq!(
            state.events,
            VecDeque::from([WindowBackendEvent::Surface(SurfaceEvent::RedrawRequested)])
        );
    }

    #[test]
    fn resize_bursts_keep_only_the_latest_size_and_one_redraw() {
        let mut state = WindowState::new(SurfaceSize {
            width: 100,
            height: 100,
        });
        state.request_redraw();

        state.resize(SurfaceSize {
            width: 200,
            height: 150,
        });
        state.resize(SurfaceSize {
            width: 320,
            height: 240,
        });

        assert_eq!(state.size.width, 320);
        assert_eq!(state.size.height, 240);
        assert_eq!(
            state.events,
            VecDeque::from([
                WindowBackendEvent::Surface(SurfaceEvent::Resized(state.size)),
                WindowBackendEvent::Surface(SurfaceEvent::RedrawRequested),
            ])
        );
    }
}
