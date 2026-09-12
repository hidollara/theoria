//! Windows platform integration and concrete Window composition.

use std::{error::Error, rc::Rc};

use crate::{
    animation::FrameClock,
    component::MountContext,
    event::EventSource,
    event_loop::{EventLoop, EventLoopHandle},
    input::{
        pointer::{MovementThreshold, PointerEvent},
        scroll::ScrollEvent,
    },
    reactive::Computed,
    surface::{Surface, SurfaceContent},
    window::CloseRequested,
};

mod component_tree;
mod event_buffer;
mod group;
mod host;
pub(crate) mod message_pump;
mod presentation;
mod presentation_target;
mod window;

pub use self::group::{WindowGroup, WindowGroupContext};
pub use self::presentation_target::Win32PresentationTarget;

use self::component_tree::WindowComponentTree;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// Standard Theoria projections exposed by the built-in Win32 Window.
///
/// These capabilities are convenient, not mandatory. A Windows integration
/// may expose additional native event sources, and applications may compose
/// their own projections directly with Theoria Event and Reactive primitives.
pub struct WindowCapabilities {
    surface: Surface,
    pointer: EventSource<PointerEvent>,
    scroll: EventSource<ScrollEvent>,
    close_requested: EventSource<CloseRequested>,
    frame_clock: FrameClock,
    movement_threshold: MovementThreshold,
}

impl WindowCapabilities {
    pub(crate) const fn new(
        surface: Surface,
        pointer: EventSource<PointerEvent>,
        scroll: EventSource<ScrollEvent>,
        close_requested: EventSource<CloseRequested>,
        frame_clock: FrameClock,
        movement_threshold: MovementThreshold,
    ) -> Self {
        Self {
            surface,
            pointer,
            scroll,
            close_requested,
            frame_clock,
            movement_threshold,
        }
    }

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

    /// Returns this Window's request-driven animation frame clock.
    pub fn frame_clock(&self) -> FrameClock {
        self.frame_clock.clone()
    }

    /// Host-provided default movement boundary for click and drag recognition.
    ///
    /// The host resolves this from its platform's input metrics. Applications
    /// may choose another policy per device.
    pub const fn movement_threshold(&self) -> MovementThreshold {
        self.movement_threshold
    }
}

/// Creates an EventLoop backed by the current thread's Win32 message queue.
///
/// The returned coordinator otherwise uses the platform-independent Theoria
/// event and reactive machinery. Constructing it here keeps native loop
/// selection out of the common [`EventLoop`] contract.
pub fn event_loop() -> EventLoop {
    EventLoop::with_pump(Box::new(message_pump::WindowsMessagePump::new()))
}

/// One Win32 Window with an independent Component update boundary.
///
/// Native observations are projected into Theoria's portable Window, Surface,
/// and input vocabulary before entering the Component tree. Windows-specific
/// integrations may expose additional event sources beside those projections.
pub struct Window {
    tree: Rc<WindowComponentTree>,
}

impl Window {
    /// Creates a hidden Win32 Window and mounts its Component tree.
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
