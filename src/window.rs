//! A component host connecting native input, reactive state, and Canvas presentation.

use std::{
    cell::{Cell, RefCell},
    error::Error,
    rc::Rc,
};

use crate::{
    Viewport,
    canvas::{Canvas, CanvasView},
    color::Color,
    component::{
        Component, ComponentContext, ComponentHandle, ComponentRuntime, EventEmitter, EventSource,
        MountContext, TurnBudget, TurnOutcome,
    },
    gesture::MovementThreshold,
    input::{pointer::PointerEvent, scroll::ScrollEvent},
    reactive::{State, Value},
    surface::{SurfaceEvent, SurfaceSize},
    vulkan::{CanvasRenderer, Presenter},
    windows::window::Win32Window,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// Notification that the native host asked to close this Window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CloseRequested;

/// Typed inputs published by one Window while its component graph is built.
pub struct WindowEvents<'window> {
    surface: &'window EventSource<SurfaceEvent>,
    pointer: &'window EventSource<PointerEvent>,
    scroll: &'window EventSource<ScrollEvent>,
    close_requested: &'window EventSource<CloseRequested>,
    size: Value<SurfaceSize>,
    movement_threshold: MovementThreshold,
}

impl WindowEvents<'_> {
    pub const fn surface(&self) -> &EventSource<SurfaceEvent> {
        self.surface
    }

    pub const fn pointer(&self) -> &EventSource<PointerEvent> {
        self.pointer
    }

    pub const fn scroll(&self) -> &EventSource<ScrollEvent> {
        self.scroll
    }

    pub const fn close_requested(&self) -> &EventSource<CloseRequested> {
        self.close_requested
    }

    /// Current drawable client-area size as a reactive value.
    pub fn size(&self) -> Value<SurfaceSize> {
        self.size.clone()
    }

    /// Host-provided default movement boundary for click and drag recognition.
    ///
    /// The Win32 host resolves this from `SM_CXDRAG` and `SM_CYDRAG` at the
    /// Window's initial DPI. Applications may choose another policy per device.
    pub const fn movement_threshold(&self) -> MovementThreshold {
        self.movement_threshold
    }
}

/// Semantic Canvas content to present in a Window.
///
/// This is still backend-independent drawing data. The renderer determines
/// its physical GPU execution after the terminal Effect snapshots it.
#[derive(Clone, Debug)]
pub struct WindowContent {
    canvas: Canvas,
    view: CanvasView,
    clear_color: Color,
}

impl WindowContent {
    pub const fn new(canvas: Canvas, view: CanvasView, clear_color: Color) -> Self {
        Self {
            canvas,
            view,
            clear_color,
        }
    }

    pub const fn canvas(&self) -> &Canvas {
        &self.canvas
    }

    pub const fn view(&self) -> CanvasView {
        self.view
    }

    pub const fn clear_color(&self) -> Color {
        self.clear_color
    }
}

/// Application-facing Window host and root component boundary.
///
/// Its concrete platform adapter is private. A Window drives one component
/// Runtime, publishes native input through typed event sources, and presents
/// the latest reactive Canvas result.
pub struct Window {
    runtime: ComponentRuntime,
    component: ComponentHandle<WindowComponent>,
    canvas_renderer: CanvasRenderer,
    presenter: Presenter,
    native: Win32Window,
    output: Rc<RenderOutput>,
}

impl Window {
    /// Creates a Window and mounts its component graph.
    ///
    /// `build` can create State, computed Values, Effects, event connections, and
    /// child components. Its return value is the Window's sole semantic output.
    pub fn new(
        title: &str,
        width: u32,
        height: u32,
        build: impl FnOnce(&mut MountContext<'_>, &WindowEvents<'_>) -> Value<WindowContent>,
    ) -> Result<Self> {
        let native = Win32Window::new(title, width, height)?;
        let presenter = Presenter::new(native.presentation_target())?;
        let canvas_renderer = CanvasRenderer::new(presenter.device());
        let runtime = ComponentRuntime::new();
        let output = Rc::new(RenderOutput::default());
        let effect_output = Rc::clone(&output);
        let initial_size = native.size();
        let (horizontal, vertical) = native.movement_threshold_extents();
        let movement_threshold = MovementThreshold::AxisAligned {
            horizontal,
            vertical,
        };

        let component = runtime.mount(move |context| {
            let size = context.state(initial_size);
            let (surface, surface_events) = context.event::<SurfaceEvent>();
            let (pointer, pointer_events) = context.event::<PointerEvent>();
            let (scroll, scroll_events) = context.event::<ScrollEvent>();
            let (close_requested, close_requests) = context.event::<CloseRequested>();
            let events = WindowEvents {
                surface: &surface,
                pointer: &pointer,
                scroll: &scroll,
                close_requested: &close_requested,
                size: size.value(),
                movement_threshold,
            };
            let content = build(context, &events);

            context.effect(move |context| {
                effect_output
                    .content
                    .replace(Some(context.read(&content).cloned()));
                effect_output.redraw_needed.set(true);
            });

            WindowComponent {
                size,
                surface_events,
                pointer_events,
                scroll_events,
                close_requests,
            }
        });
        runtime.flush_effects();

        Ok(Self {
            runtime,
            component,
            canvas_renderer,
            presenter,
            native,
            output,
        })
    }

    /// Delivers native events and presents content until the Window closes.
    pub fn run(mut self) -> Result<()> {
        self.request_redraw_if_needed();

        while let Some(event) = self.native.next_event()? {
            let redraw = matches!(
                event,
                NativeWindowEvent::Surface(SurfaceEvent::RedrawRequested)
            );
            let close = matches!(event, NativeWindowEvent::CloseRequested);

            if let NativeWindowEvent::Surface(SurfaceEvent::Resized(size)) = event {
                self.presenter.resize(size)?;
            }

            let queued = self.runtime.enqueue(&self.component, event);
            debug_assert!(queued, "the Window root remains mounted while it runs");
            let outcome = self.runtime.run_turn(TurnBudget::unlimited());
            debug_assert_eq!(outcome, TurnOutcome::Idle);

            if redraw {
                self.present_latest()?;
            } else {
                self.request_redraw_if_needed();
            }

            if close {
                break;
            }
        }

        Ok(())
    }

    fn request_redraw_if_needed(&mut self) {
        if self.output.redraw_needed.replace(false) {
            self.native.request_redraw();
        }
    }

    fn present_latest(&mut self) -> Result<()> {
        self.output.redraw_needed.set(false);
        let size = self.native.size();
        if size.width == 0 || size.height == 0 {
            return Ok(());
        }
        let content = self
            .output
            .content
            .borrow()
            .as_ref()
            .expect("the Window presentation Effect produces initial content")
            .clone();
        let viewport = Viewport::new(0.0, 0.0, size.width as f32, size.height as f32);

        if let Some(frame) = self.presenter.begin_frame()? {
            let target = frame.render_target(content.clear_color());
            self.canvas_renderer
                .render(target, viewport, content.canvas(), content.view())?;
            frame.present()?;
        }
        Ok(())
    }
}

#[derive(Default)]
struct RenderOutput {
    content: RefCell<Option<WindowContent>>,
    redraw_needed: Cell<bool>,
}

struct WindowComponent {
    size: State<SurfaceSize>,
    surface_events: EventEmitter<SurfaceEvent>,
    pointer_events: EventEmitter<PointerEvent>,
    scroll_events: EventEmitter<ScrollEvent>,
    close_requests: EventEmitter<CloseRequested>,
}

impl Component for WindowComponent {
    type Event = NativeWindowEvent;

    fn receive(&mut self, event: Self::Event, context: &mut ComponentContext<'_>) {
        match event {
            NativeWindowEvent::Surface(event) => {
                if let SurfaceEvent::Resized(size) = event {
                    context.set(&self.size, size);
                }
                let _ = context.emit(&self.surface_events, event);
            }
            NativeWindowEvent::Pointer(event) => {
                let _ = context.emit(&self.pointer_events, event);
            }
            NativeWindowEvent::Scroll(event) => {
                let _ = context.emit(&self.scroll_events, event);
            }
            NativeWindowEvent::CloseRequested => {
                let _ = context.emit(&self.close_requests, CloseRequested);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum NativeWindowEvent {
    Surface(SurfaceEvent),
    Pointer(PointerEvent),
    Scroll(ScrollEvent),
    CloseRequested,
}
