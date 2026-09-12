//! Public construction of Windows sharing one Component boundary.

use std::ops::{Deref, DerefMut};

use crate::{component::MountContext, reactive::Computed};

use super::{
    Result, WindowContent, WindowEvents,
    event_loop::WindowEventLoop,
    mounted_group::{MountedWindowGroup, WindowGroupMounts},
};

/// Windows mounted beneath one shared Component root.
///
/// All Windows created during `build` share a Reactive graph, event dispatcher,
/// and Effect synchronization boundary. Native presentation remains independent
/// per Window. For independent Component boundaries on the same native message
/// loop, create separate [`Window`](super::Window) values and use
/// [`Window::run_all`](super::Window::run_all).
pub struct WindowGroup {
    mounted: MountedWindowGroup,
}

impl WindowGroup {
    /// Builds one shared Component root and all of its initial Windows.
    pub fn new(build: impl FnOnce(&mut WindowGroupContext<'_, '_>) -> Result<()>) -> Result<Self> {
        let mut build = Some(build);
        let mounted = MountedWindowGroup::mount(|context, windows| {
            let mut context = WindowGroupContext {
                mount: context,
                windows,
            };
            build.take().expect("WindowGroup build runs once")(&mut context)
        })?;
        Ok(Self { mounted })
    }

    /// Drives native messages, Component work, and presentation until the last
    /// Window closes or the shared root Component unmounts.
    pub fn run(self) -> Result<()> {
        WindowEventLoop::new([self.mounted]).run()
    }
}

/// Construction capabilities for a shared Window group root.
///
/// Reactive values and Components created through the inherited
/// [`MountContext`] belong to the shared root. Each Window receives a distinct
/// child Component lifetime boundary beneath it.
pub struct WindowGroupContext<'context, 'mount> {
    mount: &'context mut MountContext<'mount>,
    windows: &'context mut WindowGroupMounts,
}

impl WindowGroupContext<'_, '_> {
    /// Creates a native Window and mounts its Component subtree.
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
