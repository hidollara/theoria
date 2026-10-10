//! Public construction of Windows sharing one Component boundary.

use std::{
    ops::{Deref, DerefMut},
    rc::Rc,
};

use crate::{
    component::MountContext,
    event_loop::EventLoopHandle,
    windows::{
        Result,
        capabilities::WindowCapabilities,
        composition::{WindowChildren, WindowComposition},
        native_window::Win32Window,
        presentation::WindowPresentation,
    },
};

/// Convenience ownership for one Component root with multiple Window children.
///
/// The group does not implicitly share state between its Windows. Values created
/// in `build` belong to the root Component and may be passed explicitly into its
/// Window children. Component ownership then keeps those parent values alive for
/// every dependent child. Native presentation remains independent per Window.
pub struct WindowGroup {
    composition: Rc<WindowComposition>,
}

impl WindowGroup {
    /// Builds one shared Component root and all of its initial hidden Windows.
    ///
    /// Call [`WindowGroup::show`] when the group is ready to become visible.
    pub fn new(
        event_loop: &EventLoopHandle,
        build: impl FnOnce(&mut WindowGroupContext<'_, '_>) -> Result<()>,
    ) -> Result<Self> {
        let mut build = Some(build);
        let composition = WindowComposition::mount(|context, windows| {
            let mut context = WindowGroupContext {
                mount: context,
                windows,
            };
            build.take().expect("WindowGroup build runs once")(&mut context)
        })?;
        composition.register(event_loop)?;
        Ok(Self { composition })
    }

    /// Makes every currently mounted Window in this group visible.
    pub fn show(&self) {
        self.composition.show();
    }
}

/// Construction capabilities for a shared Window group root.
///
/// Reactive values and Components created through the inherited
/// [`MountContext`] belong to the shared root. Each Window receives a distinct
/// child Component lifetime boundary beneath it.
pub struct WindowGroupContext<'context, 'mount> {
    mount: &'context mut MountContext<'mount>,
    windows: &'context mut WindowChildren,
}

impl WindowGroupContext<'_, '_> {
    /// Creates a hidden Window and mounts its subtree without rendering resources.
    pub fn mount_window_without_presentation(
        &mut self,
        title: &str,
        width: u32,
        height: u32,
        build: impl FnOnce(&mut MountContext<'_>, &WindowCapabilities),
    ) -> Result<()> {
        self.mount_window_with(
            title,
            width,
            height,
            |_| Ok(()),
            |context, capabilities, ()| {
                build(context, capabilities);
            },
        )
    }

    pub(super) fn mount_window_with<P: WindowPresentation + 'static>(
        &mut self,
        title: &str,
        width: u32,
        height: u32,
        create_presentation: impl FnOnce(&Win32Window) -> Result<P>,
        build: impl FnOnce(&mut MountContext<'_>, &WindowCapabilities, &P),
    ) -> Result<()> {
        self.windows
            .mount_window(self.mount, title, width, height, create_presentation, build)
    }
}

impl<'mount> Deref for WindowGroupContext<'_, 'mount> {
    type Target = MountContext<'mount>;

    fn deref(&self) -> &Self::Target {
        self.mount
    }
}

impl DerefMut for WindowGroupContext<'_, '_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.mount
    }
}
