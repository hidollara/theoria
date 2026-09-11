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
    vulkan::{CanvasRenderer, PresentOutcome, Presenter},
    windows::window::Win32Window,
};

mod event_loop;

use self::event_loop::WindowEventLoop;

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
    dispatcher: FifoDispatcher,
    components: ComponentInstance,
    host: WindowHost,
}

/// Native and presentation state for one Window attached to a Component tree.
pub(crate) struct WindowHost {
    native_events: EventSink<NativeWindowEvent>,
    unmount: EventSink<()>,
    presenter: Presenter,
    canvas_renderer: CanvasRenderer,
    native: Win32Window,
    output: Rc<RenderOutput>,
    presentation_pending: bool,
    close_requested: bool,
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
        let dispatcher = FifoDispatcher::new();
        let mut host = None;
        let mut result = Ok(());
        let mut build = Some(build);
        let mut components = ComponentInstance::new(dispatcher.route(), |context| {
            match WindowHost::mount(
                context,
                title,
                width,
                height,
                build.take().expect("Window build runs once"),
            ) {
                Ok(window) => host = Some(window),
                Err(error) => result = Err(error),
            }
        });
        result?;
        components.flush_effects()?;

        Ok(Self {
            dispatcher,
            components,
            host: host.expect("successful Window construction produces a host"),
        })
    }

    /// Delivers native events and presents content until the Window closes.
    pub fn run(self) -> Result<()> {
        Self::run_all([self])
    }

    /// Drives several Windows on their shared current-thread message queue.
    ///
    /// Each Window keeps an independent Component tree and presentation state.
    /// Closing one Window removes only that Window; processing ends after the
    /// final Window closes or the thread receives its quit message.
    pub fn run_all(windows: impl IntoIterator<Item = Self>) -> Result<()> {
        WindowEventLoop::new(windows).run()
    }

    /// Advances semantic Window work after the thread message pump has run.
    fn process_ready_work(&mut self, presentation_wait: Duration) -> Result<WindowProgress> {
        self.host.process_native_event()?;

        let component_work_pending = match self.drive_components(COMPONENT_WORK_TIME)? {
            ComponentProgress::Idle => false,
            ComponentProgress::Pending => true,
            ComponentProgress::Finished => return Ok(WindowProgress::Finished),
        };

        if self.host.close_requested && !component_work_pending {
            self.host.unmount();
            return Ok(WindowProgress::Finished);
        }

        self.host.advance_presentation(presentation_wait)?;

        if component_work_pending || self.host.has_ready_work() {
            Ok(WindowProgress::Pending)
        } else if self.host.has_pending_presentation() {
            Ok(WindowProgress::PresentationPending)
        } else {
            Ok(WindowProgress::Idle)
        }
    }

    fn has_ready_work(&self) -> bool {
        self.host.has_ready_work()
    }

    fn drive_components(&mut self, available_time: Duration) -> Result<ComponentProgress> {
        drive_components(&mut self.dispatcher, &mut self.components, available_time)
    }

    fn request_redraw_if_needed(&mut self) {
        self.host.request_redraw_if_needed();
    }
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

    pub(crate) fn has_ready_work(&self) -> bool {
        !self.close_requested && self.native.has_pending_event()
    }

    pub(crate) fn is_closing(&self) -> bool {
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

pub(crate) enum ComponentProgress {
    Idle,
    Pending,
    Finished,
}

#[derive(Clone, Copy)]
enum WindowProgress {
    Idle,
    Pending,
    PresentationPending,
    Finished,
}

enum PresentationProgress {
    Complete,
    Pending,
}

pub(crate) fn drive_components(
    dispatcher: &mut FifoDispatcher,
    components: &mut ComponentInstance,
    available_time: Duration,
) -> Result<ComponentProgress> {
    let started_at = Instant::now();
    loop {
        if components.is_finished() {
            return Ok(ComponentProgress::Finished);
        }
        if !dispatcher.deliver_next() {
            components.flush_effects()?;
            return Ok(ComponentProgress::Idle);
        }
        if components.is_finished() {
            return Ok(ComponentProgress::Finished);
        }
        if started_at.elapsed() >= available_time {
            components.flush_effects()?;
            return Ok(ComponentProgress::Pending);
        }
    }
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
