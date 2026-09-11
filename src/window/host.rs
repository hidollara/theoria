//! Native Window input, reactive output, and presentation integration.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};

use crate::{
    Viewport,
    animation::FrameScheduler,
    component::{Component, MountContext},
    event::{DispatchRoute, EventSink, EventSource, channel},
    gesture::MovementThreshold,
    input::{pointer::PointerEvent, scroll::ScrollEvent},
    reactive::{Computed, State},
    surface::{SurfaceEvent, SurfaceSize},
    vulkan::{CanvasRenderer, PresentOutcome, Presenter},
    windows::window::Win32Window,
};

use super::{CloseRequested, Result, WindowContent, WindowEvents, native_event::NativeWindowEvent};

/// Native and presentation state for one Window attached to a Component tree.
pub(crate) struct WindowHost {
    native_events: EventSink<NativeWindowEvent>,
    unmount: EventSink<()>,
    presenter: Presenter,
    canvas_renderer: CanvasRenderer,
    native: Win32Window,
    frames: FrameScheduler,
    output: Rc<RenderOutput>,
    presentation_pending: bool,
    close_requested: bool,
}

impl WindowHost {
    pub(crate) fn mount(
        context: &mut MountContext<'_>,
        title: &str,
        width: u32,
        height: u32,
        build: impl FnOnce(&mut MountContext<'_>, &WindowEvents<'_>) -> Computed<WindowContent>,
    ) -> Result<Self> {
        let native = Win32Window::new(title, width, height)?;
        let frames = FrameScheduler::new();
        let presenter = Presenter::new(native.presentation_target())?;
        let canvas_renderer = CanvasRenderer::new(presenter.device());
        let output = Rc::new(RenderOutput::default());
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
            let effect_output = Rc::clone(&output);
            let effect = context.create_effect(move |context| {
                effect_output
                    .content
                    .replace(Some(context.read(&content).cloned()));
                effect_output.redraw_needed.set(true);
            });
            context.sync(&effect);

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
            presenter,
            canvas_renderer,
            native,
            frames,
            output,
            presentation_pending: false,
            close_requested: false,
        })
    }

    pub(crate) fn process_native_event(&mut self) -> Result<()> {
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
            self.presentation_pending = true;
        }
        if let NativeWindowEvent::Surface(SurfaceEvent::Resized(size)) = event {
            self.presenter.resize(size)?;
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

    pub(crate) fn prepare_animation_frame(&mut self, now: Instant) {
        let size = self.native.size();
        self.frames
            .prepare(now, size.width != 0 && size.height != 0);
    }

    pub(crate) fn time_until_animation_frame(&self, now: Instant) -> Option<Duration> {
        self.frames.time_until_frame(now)
    }

    pub(crate) fn publish_due_animation_frame(&mut self, now: Instant) {
        self.frames.publish_due(now);
    }

    pub(crate) fn has_ready_work(&self) -> bool {
        !self.close_requested && self.native.has_pending_event()
    }

    pub(crate) const fn is_closing(&self) -> bool {
        self.close_requested
    }

    pub(crate) fn has_pending_presentation(&self) -> bool {
        self.presentation_pending || self.presenter.has_retired_swapchain()
    }

    pub(crate) fn unmount(&self) {
        let _ = DispatchRoute::Inline.send(&self.unmount, ());
    }

    pub(crate) fn request_redraw_if_needed(&mut self) {
        if self.output.redraw_needed.replace(false) {
            self.native.request_redraw();
        }
    }

    pub(crate) fn advance_presentation(&mut self, wait: Duration) -> Result<()> {
        if self.close_requested {
            return Ok(());
        }
        if self.presentation_pending {
            self.presentation_pending = match self.present_latest(wait)? {
                PresentationProgress::Complete => false,
                PresentationProgress::Pending => true,
            };
            self.presenter.reap_retired_swapchain(Duration::ZERO)?;
        } else {
            self.request_redraw_if_needed();
            self.presenter.reap_retired_swapchain(wait)?;
        }
        Ok(())
    }

    fn present_latest(&mut self, wait: Duration) -> Result<PresentationProgress> {
        let size = self.native.size();
        if size.width == 0 || size.height == 0 {
            self.output.redraw_needed.set(false);
            return Ok(PresentationProgress::Complete);
        }
        let content = self
            .output
            .content
            .borrow()
            .as_ref()
            .expect("the Window presentation Effect produces initial content")
            .clone();
        let viewport = Viewport::new(0.0, 0.0, size.width as f32, size.height as f32);

        let Some(frame) = self.presenter.begin_frame(wait)? else {
            return Ok(PresentationProgress::Pending);
        };
        let target = frame.render_target(content.clear_color());
        self.canvas_renderer
            .render(target, viewport, content.canvas(), content.view())?;
        let outcome = frame.present()?;
        if matches!(outcome, PresentOutcome::SwapchainInvalidated) {
            return Ok(PresentationProgress::Pending);
        }

        self.output.redraw_needed.set(false);
        Ok(PresentationProgress::Complete)
    }
}

enum PresentationProgress {
    Complete,
    Pending,
}

#[derive(Default)]
struct RenderOutput {
    content: RefCell<Option<WindowContent>>,
    redraw_needed: Cell<bool>,
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
