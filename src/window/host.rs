//! Native Window input, reactive output, and presentation integration.

use std::{
    cell::RefCell,
    rc::{Rc, Weak},
    time::{Duration, Instant},
};

use crate::{
    animation::FrameScheduler,
    component::MountContext,
    event::{DispatchRoute, EventSink},
    gesture::MovementThreshold,
    input::{pointer::PointerEvent, scroll::ScrollEvent},
    reactive::Computed,
    surface::SurfaceEvent,
    window_backend::{WindowBackend, WindowBackendEvent},
    windows::create_window,
};

use super::{
    CloseRequested, Result, WindowContent, WindowEvents, presentation::WindowPresentation,
};

/// Native and presentation state for one Window attached to a Component tree.
pub(super) struct WindowHost {
    surface_input: EventSink<SurfaceEvent>,
    pointer_input: EventSink<PointerEvent>,
    scroll_input: EventSink<ScrollEvent>,
    close_input: EventSink<CloseRequested>,
    native: Box<dyn WindowBackend>,
    frames: FrameScheduler,
    presentation: WindowPresentation,
}

impl WindowHost {
    /// Mounts one Window as a child of the Component currently being built.
    pub(super) fn mount_child(
        context: &mut MountContext<'_>,
        title: &str,
        width: u32,
        height: u32,
        build: impl FnOnce(&mut MountContext<'_>, &WindowEvents<'_>) -> Computed<WindowContent>,
    ) -> Result<Weak<RefCell<Self>>> {
        let native = create_window(title, width, height)?;
        let frames = FrameScheduler::new();
        let presentation = WindowPresentation::new(native.presentation_target())?;
        let initial_size = native.size();
        let (horizontal, vertical) = native.movement_threshold_extents();
        let movement_threshold = MovementThreshold::AxisAligned {
            horizontal,
            vertical,
        };
        let mut mounted = None;

        context.mount_with(|context| {
            let size = context.create_state(initial_size);
            let frame_clock = frames.mount(context);
            let (surface, surface_input) = context.event_source::<SurfaceEvent>();
            let (pointer, pointer_input) = context.event_source::<PointerEvent>();
            let (scroll, scroll_input) = context.event_source::<ScrollEvent>();
            let (close_requested, close_input) = context.event_source::<CloseRequested>();

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

            // Closing is ordinary Component behavior. User subscriptions made
            // by `build` are registered first on the default route; this final
            // subscription supplies the default non-cancellable close action.
            let close_window = context.receive(|_: CloseRequested, context| context.unmount());
            assert!(context.connect_cloned(&close_requested, close_window));

            let host = Rc::new(RefCell::new(Self {
                surface_input,
                pointer_input,
                scroll_input,
                close_input,
                native,
                frames,
                presentation,
            }));
            mounted = Some(Rc::downgrade(&host));
            context.retain(Rc::clone(&host));
        });

        Ok(mounted.expect("the Window Component retains its native host"))
    }

    pub(super) fn show(&self) {
        self.native.show();
    }

    pub(super) fn process_native_event(&mut self) -> Result<()> {
        let Some(event) = self.native.pop_event() else {
            return Ok(());
        };
        if matches!(
            event,
            WindowBackendEvent::Surface(SurfaceEvent::RedrawRequested)
        ) {
            self.presentation.mark_pending();
        }
        if let WindowBackendEvent::Surface(SurfaceEvent::Resized(size)) = event {
            self.presentation.resize(size)?;
        }
        match event {
            WindowBackendEvent::Surface(event) => {
                let _ = DispatchRoute::Inline.send(&self.surface_input, event);
            }
            WindowBackendEvent::Pointer(event) => {
                let _ = DispatchRoute::Inline.send(&self.pointer_input, event);
            }
            WindowBackendEvent::Scroll(event) => {
                let _ = DispatchRoute::Inline.send(&self.scroll_input, event);
            }
            WindowBackendEvent::CloseRequested => {
                let _ = DispatchRoute::Inline.send(&self.close_input, CloseRequested);
            }
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
        self.native.has_pending_event()
    }

    pub(super) fn process_scheduled_work(&mut self, now: Instant) {
        self.frames.publish_due(now);
    }

    pub(super) fn has_pending_presentation(&self) -> bool {
        self.presentation.has_pending()
    }

    pub(super) fn request_redraw_if_needed(&mut self) {
        if self.presentation.take_redraw_request() {
            self.native.request_redraw();
        }
    }

    pub(super) fn advance_presentation(&mut self, wait: Duration) -> Result<()> {
        self.request_redraw_if_needed();
        self.presentation.advance(self.native.size(), wait)?;
        Ok(())
    }
}
