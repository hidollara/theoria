//! Public construction of Windows sharing one Component boundary.

use std::{
    ops::{Deref, DerefMut},
    rc::Rc,
};

use crate::{component::MountContext, event_loop::EventLoopHandle, reactive::Computed};

use super::{
    Result, WindowContent, WindowEvents,
    component_tree::{WindowChildren, WindowComponentTree},
};

/// Convenience ownership for one Component root with multiple Window children.
///
/// The group does not implicitly share state between its Windows. Values created
/// in `build` belong to the root Component and may be passed explicitly into its
/// Window children. Component ownership then keeps those parent values alive for
/// every dependent child. Native presentation remains independent per Window.
pub struct WindowGroup {
    tree: Rc<WindowComponentTree>,
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
        let tree = WindowComponentTree::mount(|context, windows| {
            let mut context = WindowGroupContext {
                mount: context,
                windows,
            };
            build.take().expect("WindowGroup build runs once")(&mut context)
        })?;
        event_loop.register(&tree)?;
        Ok(Self { tree })
    }

    /// Makes every currently mounted Window in this group visible.
    pub fn show(&self) {
        self.tree.show();
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
    /// Creates a hidden native Window and mounts its Component subtree.
    pub fn mount_window(
        &mut self,
        title: &str,
        width: u32,
        height: u32,
        build: impl FnOnce(&mut MountContext<'_>, &WindowEvents<'_>) -> Computed<WindowContent>,
    ) -> Result<()> {
        self.windows
            .mount_window(self.mount, title, width, height, build)
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
