//! One Component tree and the native Windows owned by its children.

use std::{
    cell::RefCell,
    rc::{Rc, Weak},
    time::{Duration, Instant},
};

use crate::{
    component::MountContext, component_execution::ComponentExecution,
    event_loop::EventLoopParticipant, reactive::Computed,
};

use super::{Result, WindowContent, WindowEvents, host::WindowHost};

const COMPONENT_WORK_SLICE: Duration = Duration::from_millis(4);

/// Drives one Component tree and observes its Window children non-owningly.
pub(super) struct WindowComponentTree {
    state: RefCell<WindowComponentTreeState>,
}

impl WindowComponentTree {
    pub(super) fn mount(
        build: impl FnOnce(&mut MountContext<'_>, &mut WindowChildren) -> Result<()>,
    ) -> Result<Rc<Self>> {
        let state = WindowComponentTreeState::mount(build)?;
        let mounted = Rc::new(Self {
            state: RefCell::new(state),
        });
        mounted.state.borrow_mut().request_initial_redraws();
        Ok(mounted)
    }

    pub(super) fn show(&self) {
        for window in self.state.borrow().windows() {
            window.borrow().show();
        }
    }
}

impl EventLoopParticipant for WindowComponentTree {
    fn is_finished(&self) -> bool {
        self.state.borrow().is_finished()
    }

    fn prepare(&self, now: Instant) {
        self.state.borrow_mut().prepare_scheduled_work(now);
    }

    fn next_deadline(&self) -> Option<Instant> {
        self.state.borrow().next_deadline()
    }

    fn is_idle(&self) -> bool {
        self.state.borrow().is_idle()
    }

    fn process_ready_work(&self, now: Instant) -> Result<()> {
        self.state.borrow_mut().process_ready_work(now)
    }

    fn waitable_work_count(&self) -> usize {
        self.state.borrow().windows().count()
    }

    fn waitable_work_pending(&self, index: usize) -> bool {
        self.state
            .borrow()
            .windows()
            .nth(index)
            .is_some_and(|window| window.borrow().has_pending_presentation())
    }

    fn advance_waitable_work(&self, index: usize, wait: Duration) -> Result<()> {
        let window = self.state.borrow().windows().nth(index);
        if let Some(window) = window {
            window.borrow_mut().advance_presentation(wait)?;
        }
        Ok(())
    }
}

struct WindowComponentTreeState {
    components: ComponentExecution,
    windows: Vec<Weak<RefCell<WindowHost>>>,
}

impl WindowComponentTreeState {
    fn mount(
        build: impl FnOnce(&mut MountContext<'_>, &mut WindowChildren) -> Result<()>,
    ) -> Result<Self> {
        let mut windows = WindowChildren::default();
        let mut build = Some(build);
        let (mut components, result) = ComponentExecution::mount(|context| {
            build.take().expect("Window group build runs once")(context, &mut windows)
        });
        result?;
        components.flush_effects()?;

        Ok(Self {
            components,
            windows: windows.hosts,
        })
    }

    fn request_initial_redraws(&mut self) {
        self.prune_windows();
        for window in self.windows() {
            window.borrow_mut().request_redraw_if_needed();
        }
    }

    fn is_finished(&self) -> bool {
        self.windows.iter().all(|window| window.strong_count() == 0)
            || self.components.is_finished()
    }

    fn prepare_scheduled_work(&mut self, now: Instant) {
        self.prune_windows();
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
        self.components.is_idle()
            && self.windows().all(|window| {
                let window = window.borrow();
                !window.has_ready_work() && !window.has_pending_presentation()
            })
    }

    fn process_ready_work(&mut self, now: Instant) -> Result<()> {
        self.prune_windows();
        for window in self.windows() {
            let mut window = window.borrow_mut();
            window.process_native_event()?;
            window.process_scheduled_work(now);
        }

        let _ = self.components.advance(COMPONENT_WORK_SLICE)?;
        self.prune_windows();
        Ok(())
    }

    fn windows(&self) -> impl Iterator<Item = Rc<RefCell<WindowHost>>> + '_ {
        self.windows.iter().filter_map(Weak::upgrade)
    }

    fn prune_windows(&mut self) {
        self.windows.retain(|window| window.strong_count() != 0);
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
