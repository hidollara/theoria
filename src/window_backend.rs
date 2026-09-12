//! Internal contract between semantic Windows and platform implementations.

use std::{cell::RefCell, collections::VecDeque};

use crate::{
    PresentationTarget,
    input::{pointer::PointerEvent, scroll::ScrollEvent},
    surface::{SurfaceEvent, SurfaceSize},
};

/// Platform input translated into Theoria's Window vocabulary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum WindowBackendEvent {
    Surface(SurfaceEvent),
    Pointer(PointerEvent),
    Scroll(ScrollEvent),
    CloseRequested,
}

/// Stages native callbacks until the semantic Window can consume them safely.
///
/// Platform callbacks record events here instead of re-entering Component
/// delivery. Contiguous resize and redraw bursts are coalesced before crossing
/// that boundary because only their latest presentation state remains
/// observable; intervening input keeps its causal ordering.
pub(crate) struct WindowEventBuffer {
    events: RefCell<VecDeque<WindowBackendEvent>>,
}

impl WindowEventBuffer {
    pub(crate) fn new() -> Self {
        Self {
            events: RefCell::new(VecDeque::new()),
        }
    }

    pub(crate) fn reset_for_initial_redraw(&self) {
        self.events.borrow_mut().clear();
        self.request_redraw();
    }

    pub(crate) fn request_redraw(&self) {
        let redraw = WindowBackendEvent::Surface(SurfaceEvent::RedrawRequested);
        let mut events = self.events.borrow_mut();
        if !events.contains(&redraw) {
            events.push_back(redraw);
        }
    }

    pub(crate) fn resize(&self, size: SurfaceSize) {
        let mut events = self.events.borrow_mut();
        while events.back().is_some_and(|event| {
            matches!(
                event,
                WindowBackendEvent::Surface(
                    SurfaceEvent::Resized(_) | SurfaceEvent::RedrawRequested
                )
            )
        }) {
            events.pop_back();
        }
        events.push_back(WindowBackendEvent::Surface(SurfaceEvent::Resized(size)));
        events.push_back(WindowBackendEvent::Surface(SurfaceEvent::RedrawRequested));
    }

    pub(crate) fn push(&self, event: WindowBackendEvent) {
        self.events.borrow_mut().push_back(event);
    }

    pub(crate) fn has_pending(&self) -> bool {
        !self.events.borrow().is_empty()
    }

    pub(crate) fn pop(&self) -> Option<WindowBackendEvent> {
        self.events.borrow_mut().pop_front()
    }
}

/// Native operations needed by Theoria's platform-independent Window host.
///
/// Implementations retain any required thread affinity. This contract remains
/// private so public Window APIs expose only Theoria types.
pub(crate) trait WindowBackend {
    fn show(&self);
    fn presentation_target(&self) -> PresentationTarget;
    fn size(&self) -> SurfaceSize;
    fn movement_threshold_extents(&self) -> (f32, f32);
    fn request_redraw(&mut self);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redraw_requests_are_coalesced() {
        let events = WindowEventBuffer::new();

        events.request_redraw();
        events.request_redraw();

        assert_eq!(
            events.pop(),
            Some(WindowBackendEvent::Surface(SurfaceEvent::RedrawRequested))
        );
        assert_eq!(events.pop(), None);
    }

    #[test]
    fn resize_bursts_keep_only_the_latest_size_and_one_redraw() {
        let events = WindowEventBuffer::new();
        events.request_redraw();

        events.resize(SurfaceSize {
            width: 200,
            height: 150,
        });
        let latest = SurfaceSize {
            width: 320,
            height: 240,
        };
        events.resize(latest);

        assert_eq!(
            events.pop(),
            Some(WindowBackendEvent::Surface(SurfaceEvent::Resized(latest)))
        );
        assert_eq!(
            events.pop(),
            Some(WindowBackendEvent::Surface(SurfaceEvent::RedrawRequested))
        );
        assert_eq!(events.pop(), None);
    }

    #[test]
    fn resize_coalescing_does_not_cross_an_intervening_event() {
        let events = WindowEventBuffer::new();
        let first = SurfaceSize {
            width: 200,
            height: 150,
        };
        let second = SurfaceSize {
            width: 320,
            height: 240,
        };

        events.resize(first);
        events.push(WindowBackendEvent::CloseRequested);
        events.resize(second);

        assert_eq!(
            events.pop(),
            Some(WindowBackendEvent::Surface(SurfaceEvent::Resized(first)))
        );
        assert_eq!(
            events.pop(),
            Some(WindowBackendEvent::Surface(SurfaceEvent::RedrawRequested))
        );
        assert_eq!(events.pop(), Some(WindowBackendEvent::CloseRequested));
        assert_eq!(
            events.pop(),
            Some(WindowBackendEvent::Surface(SurfaceEvent::Resized(second)))
        );
        assert_eq!(
            events.pop(),
            Some(WindowBackendEvent::Surface(SurfaceEvent::RedrawRequested))
        );
    }
}
