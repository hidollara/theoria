//! Public Window construction and semantic content.

use std::{error::Error, rc::Rc};

use crate::{
    animation::FrameClock,
    canvas::{Canvas, CanvasView},
    color::Color,
    component::MountContext,
    event::EventSource,
    event_loop::EventLoopHandle,
    gesture::MovementThreshold,
    input::{pointer::PointerEvent, scroll::ScrollEvent},
    reactive::{Computed, Value},
    surface::{SurfaceEvent, SurfaceSize},
};

mod component_tree;
mod group;
mod host;
mod presentation;

pub use self::group::{WindowGroup, WindowGroupContext};

use self::component_tree::WindowComponentTree;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// A platform-independent request to close a Window.
///
/// Subscribers may observe the request. A Window also installs an ordinary
/// Component subscription that unmounts it as the default, non-cancellable
/// response.
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

/// One native Window with an independent Component update boundary.
///
/// Its concrete platform adapter is private. A Window publishes native input
/// through typed event sources and presents the latest reactive Canvas result.
/// This convenience type owns one Component root containing one Window child.
/// Separate values may register with the same [`EventLoopHandle`] while
/// retaining independent Component and Reactive lifetime boundaries.
pub struct Window {
    tree: Rc<WindowComponentTree>,
}

impl Window {
    /// Creates a hidden Window and mounts its Component tree.
    ///
    /// `build` can create State, Computed values, Effects, event connections, and
    /// child components. Its return value is the Window's sole semantic output.
    /// Call [`Window::show`] when the Window is ready to become visible.
    pub fn new(
        event_loop: &EventLoopHandle,
        title: &str,
        width: u32,
        height: u32,
        build: impl FnOnce(&mut MountContext<'_>, &WindowEvents<'_>) -> Computed<WindowContent>,
    ) -> Result<Self> {
        let mut build = Some(build);
        let tree = WindowComponentTree::mount(|context, windows| {
            windows.mount_window(
                context,
                title,
                width,
                height,
                build.take().expect("Window build runs once"),
            )
        })?;
        event_loop.register(&tree)?;
        Ok(Self { tree })
    }

    /// Makes this Window visible after its Component tree has been mounted.
    pub fn show(&self) {
        self.tree.show();
    }
}
