//! Owned semantic events staged by Win32 callbacks.

use std::{cell::RefCell, collections::VecDeque};

use crate::{
    input::{pointer::PointerEvent, scroll::ScrollEvent},
    surface::{SurfaceEvent, SurfaceSize},
};

/// One owned observation translated from a Win32 Window callback.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Win32WindowEvent {
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
pub(super) struct WindowEventBuffer {
    events: RefCell<VecDeque<Win32WindowEvent>>,
}

impl WindowEventBuffer {
    pub(super) fn new() -> Self {
        Self {
            events: RefCell::new(VecDeque::new()),
        }
    }

    pub(super) fn reset_for_initial_redraw(&self) {
        self.events.borrow_mut().clear();
        self.request_redraw();
    }

    pub(super) fn request_redraw(&self) {
        let redraw = Win32WindowEvent::Surface(SurfaceEvent::RedrawRequested);
        let mut events = self.events.borrow_mut();
        if events.back() != Some(&redraw) {
            events.push_back(redraw);
        }
    }

    pub(super) fn resize(&self, size: SurfaceSize) {
        let mut events = self.events.borrow_mut();
        while events.back().is_some_and(|event| {
            matches!(
                event,
                Win32WindowEvent::Surface(SurfaceEvent::Resized(_) | SurfaceEvent::RedrawRequested)
            )
        }) {
            events.pop_back();
        }
        events.push_back(Win32WindowEvent::Surface(SurfaceEvent::Resized(size)));
        events.push_back(Win32WindowEvent::Surface(SurfaceEvent::RedrawRequested));
    }

    pub(super) fn push(&self, event: Win32WindowEvent) {
        self.events.borrow_mut().push_back(event);
    }

    pub(super) fn has_pending(&self) -> bool {
        !self.events.borrow().is_empty()
    }

    pub(super) fn pop(&self) -> Option<Win32WindowEvent> {
        self.events.borrow_mut().pop_front()
    }
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
            Some(Win32WindowEvent::Surface(SurfaceEvent::RedrawRequested))
        );
        assert_eq!(events.pop(), None);
    }

    #[test]
    fn redraw_coalescing_does_not_cross_an_intervening_event() {
        let events = WindowEventBuffer::new();

        events.request_redraw();
        events.push(Win32WindowEvent::CloseRequested);
        events.request_redraw();

        assert_eq!(
            events.pop(),
            Some(Win32WindowEvent::Surface(SurfaceEvent::RedrawRequested))
        );
        assert_eq!(events.pop(), Some(Win32WindowEvent::CloseRequested));
        assert_eq!(
            events.pop(),
            Some(Win32WindowEvent::Surface(SurfaceEvent::RedrawRequested))
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
            Some(Win32WindowEvent::Surface(SurfaceEvent::Resized(latest)))
        );
        assert_eq!(
            events.pop(),
            Some(Win32WindowEvent::Surface(SurfaceEvent::RedrawRequested))
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
        events.push(Win32WindowEvent::CloseRequested);
        events.resize(second);

        assert_eq!(
            events.pop(),
            Some(Win32WindowEvent::Surface(SurfaceEvent::Resized(first)))
        );
        assert_eq!(
            events.pop(),
            Some(Win32WindowEvent::Surface(SurfaceEvent::RedrawRequested))
        );
        assert_eq!(events.pop(), Some(Win32WindowEvent::CloseRequested));
        assert_eq!(
            events.pop(),
            Some(Win32WindowEvent::Surface(SurfaceEvent::Resized(second)))
        );
        assert_eq!(
            events.pop(),
            Some(Win32WindowEvent::Surface(SurfaceEvent::RedrawRequested))
        );
    }
}
