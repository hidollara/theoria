//! A component host connecting native input, reactive state, and Canvas presentation.

use std::{
    cell::{Cell, RefCell},
    error::Error,
    rc::Rc,
    time::{Duration, Instant},
};

use crate::{
    Viewport,
    canvas::{Canvas, CanvasView},
    color::Color,
    component::{Component, ComponentInstance, MountContext},
    event::{DispatchRoute, EventSink, EventSource, FifoDispatcher, channel},
    gesture::MovementThreshold,
    input::{pointer::PointerEvent, scroll::ScrollEvent},
    reactive::{Computed, State, Value},
    surface::{SurfaceEvent, SurfaceSize},
    vulkan::{CanvasRenderer, Presenter},
    windows::{
        message_pump::{MessagePumpProgress, WindowsMessagePump},
        window::Win32Window,
    },
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

const COMPONENT_WORK_TIME: Duration = Duration::from_millis(4);

/// Notification that the native host asked to close this Window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CloseRequested;

/// Typed inputs published by one Window while its Component tree is built.
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
/// tree, publishes native input through typed event sources, and presents
/// the latest reactive Canvas result.
pub struct Window {
    message_pump: WindowsMessagePump,
    dispatcher: FifoDispatcher,
    components: ComponentInstance,
    native_events: EventSink<NativeWindowEvent>,
    canvas_renderer: CanvasRenderer,
    presenter: Presenter,
    native: Win32Window,
    output: Rc<RenderOutput>,
}

impl Window {
    /// Creates a Window and mounts its Component tree.
    ///
    /// `build` can create State, Computed values, Effects, event connections, and
    /// child components. Its return value is the Window's sole semantic output.
    pub fn new(
        title: &str,
        width: u32,
        height: u32,
        build: impl FnOnce(&mut MountContext<'_>, &WindowEvents<'_>) -> Computed<WindowContent>,
    ) -> Result<Self> {
        let message_pump = WindowsMessagePump::new();
        let native = Win32Window::new(title, width, height)?;
        let presenter = Presenter::new(native.presentation_target())?;
        let canvas_renderer = CanvasRenderer::new(presenter.device());
        let output = Rc::new(RenderOutput::default());
        let effect_output = Rc::clone(&output);
        let initial_size = native.size();
        let (horizontal, vertical) = native.movement_threshold_extents();
        let movement_threshold = MovementThreshold::AxisAligned {
            horizontal,
            vertical,
        };

        let (native_provider, native_source) = channel::<NativeWindowEvent>();
        let native_events = EventSink::from(&native_provider);
        let dispatcher = FifoDispatcher::new();
        let mut components = ComponentInstance::new(dispatcher.route(), move |context| {
            let size = context.create_state(initial_size);
            let (surface, surface_events) = context.event_source::<SurfaceEvent>();
            let (pointer, pointer_events) = context.event_source::<PointerEvent>();
            let (scroll, scroll_events) = context.event_source::<ScrollEvent>();
            let (close_requested, close_requests) = context.event_source::<CloseRequested>();
            let events = WindowEvents {
                surface: &surface,
                pointer: &pointer,
                scroll: &scroll,
                close_requested: &close_requested,
                size: size.value(),
                movement_threshold,
            };
            let content = build(context, &events);

            let effect = context.create_effect(move |context| {
                effect_output
                    .content
                    .replace(Some(context.read(&content).cloned()));
                effect_output.redraw_needed.set(true);
            });
            context.sync(&effect);

            let component = WindowComponent {
                native: native_source,
                size,
                surface_events,
                pointer_events,
                scroll_events,
                close_requests,
            };
            context.mount(component);
            context.retain(native_provider);
        });
        components.flush_effects()?;

        Ok(Self {
            message_pump,
            dispatcher,
            components,
            native_events,
            canvas_renderer,
            presenter,
            native,
            output,
        })
    }

    /// Delivers native events and presents content until the Window closes.
    pub fn run(mut self) -> Result<()> {
        self.request_redraw_if_needed();
        let mut component_work_pending = false;

        loop {
            let native_work_pending = self.native.has_pending_event();
            let progress = if component_work_pending || native_work_pending {
                self.message_pump.poll()
            } else {
                self.message_pump.wait()?
            };
            if matches!(progress, MessagePumpProgress::Finished) {
                return Ok(());
            }

            let event = self.native.pop_event();
            let redraw = event.is_some_and(|event| {
                matches!(
                    event,
                    NativeWindowEvent::Surface(SurfaceEvent::RedrawRequested)
                )
            });
            let close = matches!(event, Some(NativeWindowEvent::CloseRequested));

            if let Some(event) = event {
                if let NativeWindowEvent::Surface(SurfaceEvent::Resized(size)) = event {
                    self.presenter.resize(size)?;
                }

                if DispatchRoute::Inline
                    .send(&self.native_events, event)
                    .is_err()
                {
                    return Ok(());
                }
            }

            component_work_pending = match self.drive_components(COMPONENT_WORK_TIME)? {
                ComponentProgress::Idle => false,
                ComponentProgress::Pending => true,
                ComponentProgress::Finished => return Ok(()),
            };

            if redraw {
                self.present_latest()?;
            } else {
                self.request_redraw_if_needed();
            }

            if close {
                return Ok(());
            }
        }
    }

    fn drive_components(&mut self, available_time: Duration) -> Result<ComponentProgress> {
        let started_at = Instant::now();
        loop {
            if self.components.is_finished() {
                return Ok(ComponentProgress::Finished);
            }
            if !self.dispatcher.deliver_next() {
                self.components.flush_effects()?;
                return Ok(ComponentProgress::Idle);
            }
            if self.components.is_finished() {
                return Ok(ComponentProgress::Finished);
            }
            if started_at.elapsed() >= available_time {
                self.components.flush_effects()?;
                return Ok(ComponentProgress::Pending);
            }
        }
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

enum ComponentProgress {
    Idle,
    Pending,
    Finished,
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum NativeWindowEvent {
    Surface(SurfaceEvent),
    Pointer(PointerEvent),
    Scroll(ScrollEvent),
    CloseRequested,
}
