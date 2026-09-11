//! Component-side translation of native Window events into public event types.

use crate::{
    component::{Component, MountContext},
    event::{EventSink, EventSource},
    input::{pointer::PointerEvent, scroll::ScrollEvent},
    reactive::State,
    surface::{SurfaceEvent, SurfaceSize},
};

use super::{CloseRequested, native_event::NativeWindowEvent};

pub(super) struct WindowEventComponent {
    native: EventSource<NativeWindowEvent>,
    size: State<SurfaceSize>,
    surface_events: EventSink<SurfaceEvent>,
    pointer_events: EventSink<PointerEvent>,
    scroll_events: EventSink<ScrollEvent>,
    close_requests: EventSink<CloseRequested>,
}

impl WindowEventComponent {
    pub(super) fn new(
        native: EventSource<NativeWindowEvent>,
        size: State<SurfaceSize>,
        surface_events: EventSink<SurfaceEvent>,
        pointer_events: EventSink<PointerEvent>,
        scroll_events: EventSink<ScrollEvent>,
        close_requests: EventSink<CloseRequested>,
    ) -> Self {
        Self {
            native,
            size,
            surface_events,
            pointer_events,
            scroll_events,
            close_requests,
        }
    }
}

impl Component for WindowEventComponent {
    fn mount(self, context: &mut MountContext<'_>) {
        let Self {
            native,
            size: surface_size,
            surface_events,
            pointer_events,
            scroll_events,
            close_requests,
        } = self;
        let input = context.receive(move |event: NativeWindowEvent, context| match event {
            NativeWindowEvent::Surface(event) => {
                if let SurfaceEvent::Resized(size) = event {
                    context.set(&surface_size, size);
                }
                let _ = context.send(&surface_events, event);
            }
            NativeWindowEvent::Pointer(event) => {
                let _ = context.send(&pointer_events, event);
            }
            NativeWindowEvent::Scroll(event) => {
                let _ = context.send(&scroll_events, event);
            }
            NativeWindowEvent::CloseRequested => {
                let _ = context.send(&close_requests, CloseRequested);
            }
        });
        assert!(context.connect_cloned(&native, &input));
    }
}
