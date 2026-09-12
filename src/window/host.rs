//! Native Window input, reactive output, and presentation integration.

use std::time::{Duration, Instant};

use crate::{
    animation::FrameScheduler,
    component::MountContext,
    event::{DispatchRoute, EventSink},
    gesture::MovementThreshold,
    input::{pointer::PointerEvent, scroll::ScrollEvent},
    reactive::Computed,
    surface::SurfaceEvent,
    windows::window::Win32Window,
};

use super::{
    CloseRequested, Result, WindowContent, WindowEvents, native_event::NativeWindowEvent,
    presentation::WindowPresentation,
};

/// Native and presentation state for one Window attached to a Component tree.
pub(super) struct WindowHost {
    surface_input: EventSink<SurfaceEvent>,
    pointer_input: EventSink<PointerEvent>,
    scroll_input: EventSink<ScrollEvent>,
    close_input: EventSink<CloseRequested>,
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
        let mut surface_input = None;
        let mut pointer_input = None;
        let mut scroll_input = None;
        let mut close_input = None;
        let mut unmount = None;

        context.mount_with(|context| {
            let size = context.create_state(initial_size);
            let frame_clock = frames.mount(context);
            let (surface, input) = context.event_source::<SurfaceEvent>();
            surface_input = Some(input);
            let (pointer, input) = context.event_source::<PointerEvent>();
            pointer_input = Some(input);
            let (scroll, input) = context.event_source::<ScrollEvent>();
            scroll_input = Some(input);
            let (close_requested, input) = context.event_source::<CloseRequested>();
            close_input = Some(input);

            let surface_size = size.clone();
            let update_size = context.receive(move |event: SurfaceEvent, context| {
                if let SurfaceEvent::Resized(size) = event {
                    context.set(&surface_size, size);
                }
            });
            assert!(context.connect_cloned(&surface, update_size));

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

            unmount = Some(context.receive(|(), context| context.unmount()));
        });

        let host = Self {
            surface_input: surface_input.expect("the Window Component produces surface input"),
            pointer_input: pointer_input.expect("the Window Component produces pointer input"),
            scroll_input: scroll_input.expect("the Window Component produces scroll input"),
            close_input: close_input.expect("the Window Component produces close input"),
            unmount: unmount.expect("the Window Component produces its unmount input"),
            native,
            frames,
            presentation,
            close_requested: false,
        };
        // Expose native callbacks only after every event and presentation
        // connection needed to process them has been established.
        host.native.show();
        Ok(host)
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
        let delivered = match event {
            NativeWindowEvent::Surface(event) => DispatchRoute::Inline
                .send(&self.surface_input, event)
                .is_ok(),
            NativeWindowEvent::Pointer(event) => DispatchRoute::Inline
                .send(&self.pointer_input, event)
                .is_ok(),
            NativeWindowEvent::Scroll(event) => DispatchRoute::Inline
                .send(&self.scroll_input, event)
                .is_ok(),
            NativeWindowEvent::CloseRequested => DispatchRoute::Inline
                .send(&self.close_input, CloseRequested)
                .is_ok(),
        };
        if !delivered {
            self.close_requested = true;
        }
        Ok(())
    }

    pub(super) fn prepare_scheduled_work(&mut self, now: Instant) {
        let size = self.native.size();
        self.frames
            .prepare(now, size.width != 0 && size.height != 0);
    }

    pub(super) fn next_deadline(&self) -> Option<Instant> {
        self.frames.deadline()
    }

    pub(super) fn has_ready_work(&self) -> bool {
        !self.close_requested && self.native.has_pending_event()
    }

    pub(super) fn process_scheduled_work(&mut self, now: Instant) {
        self.frames.publish_due(now);
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
