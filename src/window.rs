//! Public Window construction and semantic content.

use std::{error::Error, rc::Rc};

use crate::{
    component::MountContext,
    event::EventSource,
    event_loop::EventLoopHandle,
    gesture::MovementThreshold,
    input::{pointer::PointerEvent, scroll::ScrollEvent},
    reactive::Computed,
    surface::{Surface, SurfaceContent},
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

/// Portable capabilities exposed by one Window during Component construction.
///
/// These are a convenient standard projection of the native host. Platform-
/// specific producers may expose additional event sources alongside them.
pub struct WindowCapabilities {
    surface: Surface,
    pointer: EventSource<PointerEvent>,
    scroll: EventSource<ScrollEvent>,
    close_requested: EventSource<CloseRequested>,
    movement_threshold: MovementThreshold,
}

impl WindowCapabilities {
    pub const fn surface(&self) -> &Surface {
        &self.surface
    }

    pub const fn pointer(&self) -> &EventSource<PointerEvent> {
        &self.pointer
    }

    pub const fn scroll(&self) -> &EventSource<ScrollEvent> {
        &self.scroll
    }

    pub const fn close_requested(&self) -> &EventSource<CloseRequested> {
        &self.close_requested
    }

    /// Host-provided default movement boundary for click and drag recognition.
    ///
    /// The host resolves this from its platform's input metrics. Applications
    /// may choose another policy per device.
    pub const fn movement_threshold(&self) -> MovementThreshold {
        self.movement_threshold
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
        build: impl FnOnce(&mut MountContext<'_>, &WindowCapabilities) -> Computed<SurfaceContent>,
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
        tree.register(event_loop)?;
        Ok(Self { tree })
    }

    /// Makes this Window visible after its Component tree has been mounted.
    pub fn show(&self) {
        self.tree.show();
    }
}
