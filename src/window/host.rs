//! Native Window input, reactive output, and presentation integration.

use std::time::{Duration, Instant};

use crate::{
    animation::FrameScheduler,
    component::{Component, MountContext},
    event::{DispatchRoute, EventSink, EventSource, channel},
    gesture::MovementThreshold,
    input::{pointer::PointerEvent, scroll::ScrollEvent},
    reactive::{Computed, State},
    surface::{SurfaceEvent, SurfaceSize},
    windows::window::Win32Window,
};

use super::{
    CloseRequested, Result, WindowContent, WindowEvents, native_event::NativeWindowEvent,
    presentation::WindowPresentation,
};

/// Native and presentation state for one Window attached to a Component tree.
pub(crate) struct WindowHost {
    native_events: EventSink<NativeWindowEvent>,
    unmount: EventSink<()>,
    native: Win32Window,
    frames: FrameScheduler,
    presentation: WindowPresentation,
    close_requested: bool,
}

impl WindowHost {
    pub(super) fn mount(
        context: &mut MountContext<'_>,
        title: &str,
        width: u32,
        height: u32,
        build: impl FnOnce(&mut MountContext<'_>, &WindowEvents<'_>) -> Computed<WindowContent>,
    ) -> Result<Self> {
        let native = Win32Window::new(title, width, height)?;
        let frames = FrameScheduler::new();
        let presentation = WindowPresentation::new(native.presentation_target())?;
        let initial_size = native.size();
        let (horizontal, vertical) = native.movement_threshold_extents();
        let movement_threshold = MovementThreshold::AxisAligned {
            horizontal,
            vertical,
        };
        let (native_provider, native_source) = channel::<NativeWindowEvent>();
        let native_events = EventSink::from(&native_provider);
        let mut unmount = None;

        context.mount_with(|context| {
            let size = context.create_state(initial_size);
            let frame_clock = frames.mount(context);
            let (surface, surface_events) = context.event_source::<SurfaceEvent>();
            let (pointer, pointer_events) = context.event_source::<PointerEvent>();
            let (scroll, scroll_events) = context.event_source::<ScrollEvent>();
            let (close_requested, close_requests) = context.event_source::<CloseRequested>();
            let events = WindowEvents {
                surface: &surface,
                pointer: &pointer,
                scroll: &scroll,
                close_requested: &close_requested,
                frame_clock: &frame_clock,
                size: size.value(),
                movement_threshold,
            };
            let content = build(context, &events);
            presentation.bind(context, content);

            context.mount(WindowComponent {
                native: native_source,
                size,
                surface_events,
                pointer_events,
                scroll_events,
                close_requests,
            });
            context.retain(native_provider);
            unmount = Some(context.receive(|(), context| context.unmount()));
        });

        Ok(Self {
            native_events,
            unmount: unmount.expect("the Window Component produces its unmount input"),
            native,
            frames,
            presentation,
            close_requested: false,
        })
    }

    pub(super) fn process_native_event(&mut self) -> Result<()> {
        if self.close_requested {
            return Ok(());
        }
        let Some(event) = self.native.pop_event() else {
            return Ok(());
        };
        if matches!(
            event,
            NativeWindowEvent::Surface(SurfaceEvent::RedrawRequested)
        ) {
            self.presentation.mark_pending();
        }
        if let NativeWindowEvent::Surface(SurfaceEvent::Resized(size)) = event {
            self.presentation.resize(size)?;
        }
        if matches!(event, NativeWindowEvent::CloseRequested) {
            self.close_requested = true;
        }
        if DispatchRoute::Inline
            .send(&self.native_events, event)
            .is_err()
        {
            self.close_requested = true;
        }
        Ok(())
    }

    fn schedule_requested_frame(&mut self, now: Instant) {
        let size = self.native.size();
        self.frames
            .prepare(now, size.width != 0 && size.height != 0);
    }

    pub(super) fn prepare_scheduled_work(&mut self, now: Instant) {
        self.schedule_requested_frame(now);
    }

    pub(super) fn next_deadline(&self) -> Option<Instant> {
        self.frames.deadline()
    }

    fn publish_due_frame(&mut self, now: Instant) {
        self.frames.publish_due(now);
    }

    pub(super) fn has_ready_work(&self) -> bool {
        !self.close_requested && self.native.has_pending_event()
    }

    pub(super) fn process_scheduled_work(&mut self, now: Instant) {
        self.publish_due_frame(now);
    }

    pub(super) const fn is_closing(&self) -> bool {
        self.close_requested
    }

    pub(super) fn has_pending_presentation(&self) -> bool {
        self.presentation.has_pending()
    }

    pub(super) fn unmount(&self) {
        let _ = DispatchRoute::Inline.send(&self.unmount, ());
    }

    pub(super) fn request_redraw_if_needed(&mut self) {
        if self.presentation.take_redraw_request() {
            self.native.request_redraw();
        }
    }

    pub(super) fn advance_presentation(&mut self, wait: Duration) -> Result<()> {
        if !self.close_requested {
            self.request_redraw_if_needed();
            self.presentation.advance(self.native.size(), wait)?;
        }
        Ok(())
    }
}

struct WindowComponent {
    native: EventSource<NativeWindowEvent>,
    size: State<SurfaceSize>,
    surface_events: EventSink<SurfaceEvent>,
    pointer_events: EventSink<PointerEvent>,
    scroll_events: EventSink<ScrollEvent>,
    close_requests: EventSink<CloseRequested>,
}

impl Component for WindowComponent {
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
