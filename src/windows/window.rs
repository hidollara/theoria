//! Public construction of one Win32 Window composition.

use std::rc::Rc;

use crate::{
    component::MountContext,
    event_loop::EventLoopHandle,
    windows::{
        Result, capabilities::WindowCapabilities, composition::WindowComposition,
        native_window::Win32Window, presentation::WindowPresentation,
    },
};

/// One Win32 Window with an independent Component update boundary.
///
/// Native observations are projected into Theoria's portable Surface and input
/// vocabulary before entering the Component tree. The same Window also exposes
/// its raw Win32 messages for integrations that need another interpretation.
pub struct Window {
    composition: Rc<WindowComposition>,
}

impl Window {
    /// Creates a hidden Win32 Window without allocating rendering resources.
    ///
    /// `build` can create State, Computed values, Effects, event connections, and
    /// child components. Input, native messages, size, and frame scheduling are
    /// available independently of a rendering backend.
    /// Call [`Window::show`] when the Window is ready to become visible.
    pub fn without_presentation(
        event_loop: &EventLoopHandle,
        title: &str,
        width: u32,
        height: u32,
        build: impl FnOnce(&mut MountContext<'_>, &WindowCapabilities),
    ) -> Result<Self> {
        Self::mount(
            event_loop,
            title,
            width,
            height,
            |_| Ok(()),
            |context, capabilities, ()| {
                build(context, capabilities);
            },
        )
    }

    pub(super) fn mount<P: WindowPresentation + 'static>(
        event_loop: &EventLoopHandle,
        title: &str,
        width: u32,
        height: u32,
        create_presentation: impl FnOnce(&Win32Window) -> Result<P>,
        build: impl FnOnce(&mut MountContext<'_>, &WindowCapabilities, &P),
    ) -> Result<Self> {
        let mut build = Some(build);
        let composition = WindowComposition::mount(|context, windows| {
            windows.mount_window(
                context,
                title,
                width,
                height,
                create_presentation,
                build.take().expect("Window build runs once"),
            )
        })?;
        composition.register(event_loop)?;
        Ok(Self { composition })
    }

    /// Makes this Window visible after its Component tree has been mounted.
    pub fn show(&self) {
        self.composition.show();
    }
}
