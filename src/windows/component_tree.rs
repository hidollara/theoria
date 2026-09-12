//! One Component tree and the native Windows owned by its children.

use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
    time::{Duration, Instant},
};

use crate::{
    component::MountContext,
    event_loop::{ComponentParticipant, EventLoopHandle, EventLoopParticipant},
    reactive::Computed,
    surface::SurfaceContent,
};

use crate::windows::{
    Result, capabilities::WindowCapabilities, host::WindowHost,
};

/// Owns one Component tree together with the Window producers built beneath it.
pub(super) struct WindowComponentTree {
    components: Rc<ComponentParticipant>,
    windows: Rc<WindowHosts>,
}

impl WindowComponentTree {
    pub(super) fn mount(
        build: impl FnOnce(&mut MountContext<'_>, &mut WindowChildren) -> Result<()>,
    ) -> Result<Rc<Self>> {
        let mut windows = WindowChildren::default();
        let mut build = Some(build);
        let (components, result) = ComponentParticipant::mount(|context| {
            build.take().expect("Window group build runs once")(context, &mut windows)
        });
        result?;
        components.flush_effects()?;

        let mounted = Rc::new(Self {
            components: Rc::new(components),
            windows: Rc::new(WindowHosts {
                hosts: RefCell::new(windows.hosts),
                next_waitable: Cell::new(0),
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
    next_waitable: Cell<usize>,
}

impl WindowHosts {
    fn request_initial_redraws(&self) {
        self.prune();
        for window in self.hosts.borrow().iter().filter_map(Weak::upgrade) {
            window.borrow_mut().request_redraw_if_needed();
        }
    }

    fn show(&self) {
        for window in self.hosts.borrow().iter().filter_map(Weak::upgrade) {
            window.borrow().show();
        }
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
        for window in self.hosts.borrow().iter().filter_map(Weak::upgrade) {
            window.borrow_mut().prepare_scheduled_work(now);
        }
    }

    fn next_deadline(&self) -> Option<Instant> {
        self.hosts
            .borrow()
            .iter()
            .filter_map(Weak::upgrade)
            .filter_map(|window| window.borrow().next_deadline())
            .min()
    }

    fn is_idle(&self) -> bool {
        self.hosts
            .borrow()
            .iter()
            .filter_map(Weak::upgrade)
            .all(|window| {
                let window = window.borrow();
                !window.has_ready_work() && !window.has_pending_presentation()
            })
    }

    fn process_ready_work(&self, now: Instant, _available_time: Duration) -> Result<()> {
        self.prune();
        {
            let hosts = self.hosts.borrow();
            for window in hosts.iter().filter_map(Weak::upgrade) {
                let mut window = window.borrow_mut();
                window.process_native_event()?;
                window.process_scheduled_work(now);
            }
        }
        self.prune();
        Ok(())
    }

    fn has_waitable_work(&self) -> bool {
        self.hosts
            .borrow()
            .iter()
            .filter_map(Weak::upgrade)
            .any(|window| window.borrow().has_pending_presentation())
    }

    fn advance_waitable_work(&self, wait: Duration) -> Result<()> {
        let hosts = self.hosts.borrow();
        if hosts.is_empty() {
            return Ok(());
        }
        let start = self.next_waitable.get() % hosts.len();
        let selected = (start..hosts.len()).chain(0..start).find(|&index| {
            hosts[index]
                .upgrade()
                .is_some_and(|window| window.borrow().has_pending_presentation())
        });
        if let Some(index) = selected {
            self.next_waitable.set(index + 1);
        }
        for (index, host) in hosts.iter().enumerate() {
            if let Some(window) = host.upgrade() {
                let window_wait = if Some(index) == selected {
                    wait
                } else {
                    Duration::ZERO
                };
                window.borrow_mut().advance_presentation(window_wait)?;
            }
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
        build: impl FnOnce(&mut MountContext<'_>, &WindowCapabilities) -> Computed<SurfaceContent>,
    ) -> Result<()> {
        self.hosts.push(WindowHost::mount_child(
            context, title, width, height, build,
        )?);
        Ok(())
    }
}
