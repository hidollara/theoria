//! One Component update boundary shared by one or more Windows.

use std::time::Instant;

use crate::{
    component::{ComponentInstance, MountContext},
    event::FifoDispatcher,
    reactive::Computed,
};

use super::{
    Result, WindowContent, WindowEvents,
    drive::{COMPONENT_WORK_SLICE, ComponentProgress, drive_components},
    host::WindowHost,
};

pub(crate) struct WindowGroup {
    dispatcher: FifoDispatcher,
    components: ComponentInstance,
    windows: Vec<WindowHost>,
}

impl WindowGroup {
    pub(crate) fn mount(
        build: impl FnOnce(&mut MountContext<'_>, &mut WindowGroupBuilder) -> Result<()>,
    ) -> Result<Self> {
        let dispatcher = FifoDispatcher::new();
        let mut builder = WindowGroupBuilder::default();
        let mut result = None;
        let mut build = Some(build);
        let mut components = ComponentInstance::new(dispatcher.route(), |context| {
            result = Some(build.take().expect("WindowGroup build runs once")(
                context,
                &mut builder,
            ));
        });
        result.expect("the WindowGroup build runs once")?;
        components.flush_effects()?;

        Ok(Self {
            dispatcher,
            components,
            windows: builder.windows,
        })
    }

    pub(super) fn request_initial_redraws(&mut self) {
        for window in &mut self.windows {
            window.request_redraw_if_needed();
        }
    }

    pub(super) fn is_finished(&self) -> bool {
        self.windows.is_empty() || self.components.is_finished()
    }

    pub(super) fn prepare_scheduled_work(&mut self, now: Instant) {
        for window in &mut self.windows {
            window.prepare_scheduled_work(now);
        }
    }

    pub(super) fn next_deadline(&self) -> Option<Instant> {
        self.windows
            .iter()
            .filter_map(WindowHost::next_deadline)
            .min()
    }

    pub(super) fn is_idle(&self) -> bool {
        !self.dispatcher.has_pending()
            && self
                .windows
                .iter()
                .all(|window| !window.has_ready_work() && !window.has_pending_presentation())
    }

    pub(super) fn process_ready_work(&mut self, now: Instant) -> Result<bool> {
        for window in &mut self.windows {
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
            ComponentProgress::Pending => return Ok(true),
            ComponentProgress::Finished => return Ok(false),
        }

        for window in self.windows.iter().filter(|window| window.is_closing()) {
            window.unmount();
        }
        self.windows.retain(|window| !window.is_closing());
        Ok(!self.windows.is_empty())
    }

    pub(super) fn windows(&self) -> &[WindowHost] {
        &self.windows
    }

    pub(super) fn windows_mut(&mut self) -> &mut [WindowHost] {
        &mut self.windows
    }
}

#[derive(Default)]
pub(crate) struct WindowGroupBuilder {
    windows: Vec<WindowHost>,
}

impl WindowGroupBuilder {
    pub(crate) fn mount_window(
        &mut self,
        context: &mut MountContext<'_>,
        title: &str,
        width: u32,
        height: u32,
        build: impl FnOnce(&mut MountContext<'_>, &WindowEvents<'_>) -> Computed<WindowContent>,
    ) -> Result<()> {
        self.windows
            .push(WindowHost::mount(context, title, width, height, build)?);
        Ok(())
    }
}
