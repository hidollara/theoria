//! Public construction of one Win32 Window composition.

use std::rc::Rc;

use crate::{
    component::MountContext,
    event_loop::EventLoopHandle,
    reactive::Computed,
    surface::SurfaceContent,
    windows::{Result, capabilities::WindowCapabilities, component_tree::WindowComponentTree},
};

/// One Win32 Window with an independent Component update boundary.
///
/// Native observations are projected into Theoria's portable Surface and input
/// vocabulary before entering the Component tree. Windows-specific integrations
/// may expose additional event sources beside those projections.
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
