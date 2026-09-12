//! Native Window input, reactive output, and presentation integration.

use std::{
    cell::RefCell,
    rc::{Rc, Weak},
    time::{Duration, Instant},
};

use crate::{
    animation::FrameScheduler,
    component::MountContext,
    event::{DispatchOutcome, DispatchRoute, EventSink, SubmissionOutcome},
    gesture::MovementThreshold,
    input::{pointer::PointerEvent, scroll::ScrollEvent},
    reactive::Computed,
    surface::SurfaceEvent,
    window_backend::{WindowBackend, WindowBackendEvent, WindowEventBuffer},
    windows::create_window,
};

use super::{
    CloseRequested, Result, WindowContent, WindowEvents, presentation::WindowPresentation,
};

/// Native and presentation state for one Window attached to a Component tree.
pub(super) struct WindowHost {
    inputs: WindowInputs,
    events: Rc<WindowEventBuffer>,
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
        let backend_events = Rc::new(WindowEventBuffer::new());
        let native = create_window(title, width, height, Rc::clone(&backend_events))?;
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
                inputs: WindowInputs {
                    surface: surface_input,
                    pointer: pointer_input,
                    scroll: scroll_input,
                    close_requested: close_input,
                },
                events: backend_events,
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
        let Some(event) = self.events.pop() else {
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
        self.inputs.publish(event);
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
        self.events.has_pending()
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

/// Component event inputs associated with one live native Window.
struct WindowInputs {
    surface: EventSink<SurfaceEvent>,
    pointer: EventSink<PointerEvent>,
    scroll: EventSink<ScrollEvent>,
    close_requested: EventSink<CloseRequested>,
}

impl WindowInputs {
    fn publish(&self, event: WindowBackendEvent) {
        match event {
            WindowBackendEvent::Surface(event) => Self::publish_to(&self.surface, event),
            WindowBackendEvent::Pointer(event) => Self::publish_to(&self.pointer, event),
            WindowBackendEvent::Scroll(event) => Self::publish_to(&self.scroll, event),
            WindowBackendEvent::CloseRequested => {
                Self::publish_to(&self.close_requested, CloseRequested);
            }
        }
    }

    fn publish_to<E: 'static>(sink: &EventSink<E>, event: E) {
        let outcome = DispatchRoute::Inline.send(sink, event);
        debug_assert!(matches!(
            outcome,
            Ok(SubmissionOutcome::Completed(DispatchOutcome::Delivered))
        ));
    }
}
