//! Public Window construction and semantic content.

use std::error::Error;

use crate::{
    animation::FrameClock,
    canvas::{Canvas, CanvasView},
    color::Color,
    component::MountContext,
    event::EventSource,
    gesture::MovementThreshold,
    input::{pointer::PointerEvent, scroll::ScrollEvent},
    reactive::{Computed, Value},
    surface::{SurfaceEvent, SurfaceSize},
};

pub(crate) mod event_loop;
pub(crate) mod group;
pub(crate) mod host;
pub(crate) mod native_event;

use self::{event_loop::WindowEventLoop, group::WindowGroup, host::WindowHost};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// Notification that the native host asked to close this Window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CloseRequested;

/// Typed inputs published by one Window while its Component tree is built.
pub struct WindowEvents<'window> {
    surface: &'window EventSource<SurfaceEvent>,
    pointer: &'window EventSource<PointerEvent>,
    scroll: &'window EventSource<ScrollEvent>,
    close_requested: &'window EventSource<CloseRequested>,
    frame_clock: &'window FrameClock,
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

    /// Returns this Window's request-driven animation frame clock.
    pub fn frame_clock(&self) -> FrameClock {
        self.frame_clock.clone()
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
    group: WindowGroup,
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
        let mut build = Some(build);
        let group = WindowGroup::mount(|context, windows| {
            windows.push(WindowHost::mount(
                context,
                title,
                width,
                height,
                build.take().expect("Window build runs once"),
            )?);
            Ok(())
        })?;
        Ok(Self { group })
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
        let groups = windows.into_iter().map(|window| window.group);
        WindowEventLoop::new(groups).run()
    }
}
