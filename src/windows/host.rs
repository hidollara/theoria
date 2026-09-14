//! Native Window input, reactive output, and presentation integration.

use std::{
    cell::RefCell,
    rc::{Rc, Weak},
    time::{Duration, Instant},
};

use crate::{
    animation::FrameScheduler,
    component::MountContext,
    event::EventSink,
    input::{
        pointer::{MovementThreshold, PointerEvent},
        scroll::ScrollEvent,
    },
    reactive::Computed,
    surface::{Surface, SurfaceContent, SurfaceEvent},
    window::CloseRequested,
};

use crate::windows::{
    Result,
    capabilities::WindowCapabilities,
    event_buffer::{Win32WindowEvent, WindowEventBuffer},
    native_window::Win32Window,
    presentation::WindowPresentation,
};

/// Native and presentation state for one Window attached to a Component tree.
pub(super) struct WindowHost {
    inputs: WindowInputs,
    events: Rc<WindowEventBuffer>,
    native: Win32Window,
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
        build: impl FnOnce(&mut MountContext<'_>, &WindowCapabilities) -> Computed<SurfaceContent>,
    ) -> Result<Weak<RefCell<Self>>> {
        let native_events = Rc::new(WindowEventBuffer::new());
        let native = Win32Window::new(title, width, height, Rc::clone(&native_events))?;
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
            assert!(context.connect(surface.cloned(), update_size));

            let capabilities = WindowCapabilities::new(
                Surface::new(surface, size.value()),
                pointer,
                scroll,
                close_requested,
                frame_clock,
                movement_threshold,
            );
            let content = build(context, &capabilities);
            presentation.bind(context, content);

            // Closing is ordinary Component behavior. User subscriptions made
            // by `build` are registered first on the default Dispatcher; this final
            // subscription supplies the default non-cancellable close action.
            let close_window = context.receive(|_: CloseRequested, context| context.unmount());
            assert!(context.connect(capabilities.close_requested().cloned(), close_window));

            let host = Rc::new(RefCell::new(Self {
                inputs: WindowInputs {
                    surface: surface_input,
                    pointer: pointer_input,
                    scroll: scroll_input,
                    close_requested: close_input,
                },
                events: native_events,
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

    pub(super) fn process_native_event(&mut self) {
        let Some(event) = self.events.pop() else {
            return;
        };
        if matches!(
            event,
            Win32WindowEvent::Surface(SurfaceEvent::RedrawRequested)
        ) {
            self.presentation.mark_pending();
        }
        if let Win32WindowEvent::Surface(SurfaceEvent::Resized(size)) = event {
            self.presentation.resize(size);
        }
        self.inputs.publish(event);
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
    fn publish(&self, event: Win32WindowEvent) {
        match event {
            Win32WindowEvent::Surface(event) => Self::publish_to(&self.surface, event),
            Win32WindowEvent::Pointer(event) => Self::publish_to(&self.pointer, event),
            Win32WindowEvent::Scroll(event) => Self::publish_to(&self.scroll, event),
            Win32WindowEvent::CloseRequested => {
                Self::publish_to(&self.close_requested, CloseRequested);
            }
        }
    }

    fn publish_to<E: 'static>(sink: &EventSink<E>, event: E) {
        debug_assert!(sink.send_inline(event).is_ok());
    }
}
