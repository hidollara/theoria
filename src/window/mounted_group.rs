//! Component work and Window registrations sharing one update boundary.

use std::{
    cell::RefCell,
    rc::{Rc, Weak},
    time::{Duration, Instant},
};

use crate::{
    component::{ComponentInstance, MountContext},
    event::FifoDispatcher,
    event_loop::EventLoopParticipant,
    reactive::Computed,
};

use super::{
    Result, WindowContent, WindowEvents,
    drive::{COMPONENT_WORK_SLICE, ComponentProgress, drive_components},
    host::WindowHost,
};

/// One independently owned Window group registered non-owningly for driving.
pub(super) struct MountedWindowGroup {
    state: RefCell<WindowGroupState>,
}

impl MountedWindowGroup {
    pub(super) fn mount(
        build: impl FnOnce(&mut MountContext<'_>, &mut WindowGroupMounts) -> Result<()>,
    ) -> Result<Rc<Self>> {
        let state = WindowGroupState::mount(build)?;
        let mounted = Rc::new(Self {
            state: RefCell::new(state),
        });
        mounted.state.borrow_mut().request_initial_redraws();
        Ok(mounted)
    }
}

impl EventLoopParticipant for MountedWindowGroup {
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

struct WindowGroupState {
    dispatcher: FifoDispatcher,
    components: ComponentInstance,
    windows: Vec<Weak<RefCell<WindowHost>>>,
}

impl WindowGroupState {
    fn mount(
        build: impl FnOnce(&mut MountContext<'_>, &mut WindowGroupMounts) -> Result<()>,
    ) -> Result<Self> {
        let dispatcher = FifoDispatcher::new();
        let mut windows = WindowGroupMounts::default();
        let mut result = None;
        let mut build = Some(build);
        let mut components = ComponentInstance::new(dispatcher.route(), |context| {
            result = Some(build.take().expect("Window group build runs once")(
                context,
                &mut windows,
            ));
        });
        result.expect("the Window group build runs once")?;
        components.flush_effects()?;

        Ok(Self {
            dispatcher,
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
        !self.dispatcher.has_pending()
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

        let progress = drive_components(
            &mut self.dispatcher,
            &mut self.components,
            COMPONENT_WORK_SLICE,
        )?;
        match progress {
            ComponentProgress::Idle => {}
            ComponentProgress::Pending | ComponentProgress::Finished => return Ok(()),
        }

        let closing: Vec<_> = self
            .windows()
            .filter(|window| window.borrow().is_closing())
            .collect();
        for window in closing {
            window.borrow().unmount();
        }
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
pub(super) struct WindowGroupMounts {
    hosts: Vec<Weak<RefCell<WindowHost>>>,
}

impl WindowGroupMounts {
    pub(super) fn mount_window(
        &mut self,
        context: &mut MountContext<'_>,
        title: &str,
        width: u32,
        height: u32,
        build: impl FnOnce(&mut MountContext<'_>, &WindowEvents<'_>) -> Computed<WindowContent>,
    ) -> Result<()> {
        self.hosts
            .push(WindowHost::mount(context, title, width, height, build)?);
        Ok(())
    }
}
