//! Application-level ownership for Component state shared by multiple Windows.

use std::{
    error::Error,
    ops::{Deref, DerefMut},
};

use crate::{
    component::MountContext,
    reactive::Computed,
    window::{
        WindowContent, WindowEvents, event_loop::WindowEventLoop, group::WindowGroup,
        host::WindowHost,
    },
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// One Component tree and the native Windows mounted beneath its root.
///
/// All Windows created during `build` share this Application's Reactive graph,
/// event dispatcher, and Effect synchronization boundary. Native presentation
/// remains independent per Window.
pub struct Application {
    group: WindowGroup,
}

impl Application {
    /// Builds one shared Component root and all of its initial Windows.
    pub fn new(build: impl FnOnce(&mut ApplicationContext<'_, '_>) -> Result<()>) -> Result<Self> {
        let mut build = Some(build);
        let group = WindowGroup::mount(|context, windows| {
            let mut context = ApplicationContext {
                mount: context,
                windows,
            };
            build.take().expect("Application build runs once")(&mut context)
        })?;
        Ok(Self { group })
    }

    /// Drives native messages, Component work, and presentation until the last
    /// Window closes or the root Component unmounts.
    pub fn run(self) -> Result<()> {
        WindowEventLoop::new([self.group]).run()
    }
}

/// Construction capabilities for one Application root.
///
/// Reactive values and Components created through the inherited
/// [`MountContext`] belong to the shared root. Each Window receives a distinct
/// child Component lifetime boundary beneath it.
pub struct ApplicationContext<'context, 'mount> {
    mount: &'context mut MountContext<'mount>,
    windows: &'context mut Vec<WindowHost>,
}

impl ApplicationContext<'_, '_> {
    /// Creates a native Window and mounts its Component subtree.
    pub fn mount_window(
        &mut self,
        title: &str,
        width: u32,
        height: u32,
        build: impl FnOnce(&mut MountContext<'_>, &WindowEvents<'_>) -> Computed<WindowContent>,
    ) -> Result<()> {
        let window = WindowHost::mount(self.mount, title, width, height, build)?;
        self.windows.push(window);
        Ok(())
    }
}

impl<'mount> Deref for ApplicationContext<'_, 'mount> {
    type Target = MountContext<'mount>;

    fn deref(&self) -> &Self::Target {
        self.mount
    }
}

impl DerefMut for ApplicationContext<'_, '_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.mount
    }
}
