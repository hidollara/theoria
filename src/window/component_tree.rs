//! One Component tree and the native Windows owned by its children.

use std::{
    cell::RefCell,
    rc::{Rc, Weak},
    time::{Duration, Instant},
};

use crate::{
    component::MountContext,
    component_execution::ComponentExecution,
    event_loop::{EventLoopHandle, EventLoopParticipant},
    reactive::Computed,
};

use super::{Result, WindowContent, WindowEvents, host::WindowHost};

/// Owns one Component tree together with the Window producers built beneath it.
pub(super) struct WindowComponentTree {
    components: Rc<ComponentExecution>,
    windows: Rc<WindowHosts>,
}

impl WindowComponentTree {
    pub(super) fn mount(
        build: impl FnOnce(&mut MountContext<'_>, &mut WindowChildren) -> Result<()>,
    ) -> Result<Rc<Self>> {
        let mut windows = WindowChildren::default();
        let mut build = Some(build);
        let (components, result) = ComponentExecution::mount(|context| {
            build.take().expect("Window group build runs once")(context, &mut windows)
        });
        result?;
        components.flush_effects()?;

        let mounted = Rc::new(Self {
            components: Rc::new(components),
            windows: Rc::new(WindowHosts {
                hosts: RefCell::new(windows.hosts),
            }),
        });
        mounted.windows.request_initial_redraws();
        Ok(mounted)
    }

    pub(super) fn register(&self, event_loop: &EventLoopHandle) -> Result<()> {
        // Producers run before the passive Component processor so native input
        // can reach Component handlers during the same EventLoop iteration.
        event_loop.register(&self.windows)?;
        event_loop.register(&self.components)?;
        Ok(())
    }

    pub(super) fn show(&self) {
        self.windows.show();
    }
}

/// Native Window producers and presentation work observed non-owningly.
struct WindowHosts {
    hosts: RefCell<Vec<Weak<RefCell<WindowHost>>>>,
}

impl WindowHosts {
    fn request_initial_redraws(&self) {
        self.prune();
        for window in self.windows() {
            window.borrow_mut().request_redraw_if_needed();
        }
    }

    fn show(&self) {
        for window in self.windows() {
            window.borrow().show();
        }
    }

    fn windows(&self) -> impl Iterator<Item = Rc<RefCell<WindowHost>>> + '_ {
        self.hosts
            .borrow()
            .clone()
            .into_iter()
            .filter_map(|window| window.upgrade())
    }

    fn prune(&self) {
        self.hosts
            .borrow_mut()
            .retain(|window| window.strong_count() != 0);
    }

    fn has_windows(&self) -> bool {
        self.hosts
            .borrow()
            .iter()
            .any(|window| window.strong_count() != 0)
    }
}

impl EventLoopParticipant for WindowHosts {
    fn is_closed(&self) -> bool {
        !self.has_windows()
    }

    fn keeps_alive(&self) -> bool {
        self.has_windows()
    }

    fn prepare(&self, now: Instant) {
        self.prune();
        for window in self.windows() {
            window.borrow_mut().prepare_scheduled_work(now);
        }
    }

    fn next_deadline(&self) -> Option<Instant> {
        self.windows()
            .filter_map(|window| window.borrow().next_deadline())
            .min()
    }

    fn is_idle(&self) -> bool {
        self.windows().all(|window| {
            let window = window.borrow();
            !window.has_ready_work() && !window.has_pending_presentation()
        })
    }

    fn process_ready_work(&self, now: Instant) -> Result<()> {
        self.prune();
        for window in self.windows() {
            let mut window = window.borrow_mut();
            window.process_native_event()?;
            window.process_scheduled_work(now);
        }
        self.prune();
        Ok(())
    }

    fn waitable_work_count(&self) -> usize {
        self.windows().count()
    }

    fn waitable_work_pending(&self, index: usize) -> bool {
        self.windows()
            .nth(index)
            .is_some_and(|window| window.borrow().has_pending_presentation())
    }

    fn advance_waitable_work(&self, index: usize, wait: Duration) -> Result<()> {
        let window = self.windows().nth(index);
        if let Some(window) = window {
            window.borrow_mut().advance_presentation(wait)?;
        }
        Ok(())
    }
}

#[derive(Default)]
pub(super) struct WindowChildren {
    hosts: Vec<Weak<RefCell<WindowHost>>>,
}

impl WindowChildren {
    pub(super) fn mount_window(
        &mut self,
        context: &mut MountContext<'_>,
        title: &str,
        width: u32,
        height: u32,
        build: impl FnOnce(&mut MountContext<'_>, &WindowEvents<'_>) -> Computed<WindowContent>,
    ) -> Result<()> {
        self.hosts.push(WindowHost::mount_child(
            context, title, width, height, build,
        )?);
        Ok(())
    }
}
