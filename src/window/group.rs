//! One Component update boundary shared by one or more Windows.

use std::time::{Duration, Instant};

use crate::{
    component::{ComponentInstance, MountContext},
    drive::{COMPONENT_WORK_SLICE, ComponentProgress, drive_components},
    event::FifoDispatcher,
};

use super::{Result, host::WindowHost};

pub(crate) struct WindowGroup {
    dispatcher: FifoDispatcher,
    components: ComponentInstance,
    windows: Vec<WindowHost>,
}

impl WindowGroup {
    pub(crate) fn mount(
        build: impl FnOnce(&mut MountContext<'_>, &mut Vec<WindowHost>) -> Result<()>,
    ) -> Result<Self> {
        let dispatcher = FifoDispatcher::new();
        let mut windows = Vec::new();
        let mut result = None;
        let mut build = Some(build);
        let mut components = ComponentInstance::new(dispatcher.route(), |context| {
            result = Some(build.take().expect("WindowGroup build runs once")(
                context,
                &mut windows,
            ));
        });
        result.expect("the WindowGroup build runs once")?;
        components.flush_effects()?;

        Ok(Self {
            dispatcher,
            components,
            windows,
        })
    }

    pub(super) fn request_initial_redraws(&mut self) {
        for window in &mut self.windows {
            window.request_redraw_if_needed();
        }
    }

    pub(super) fn prepare_animation_frames(&mut self, now: Instant) {
        for window in &mut self.windows {
            window.prepare_animation_frame(now);
        }
    }

    pub(super) fn time_until_animation_frame(&self, now: Instant) -> Option<Duration> {
        self.windows
            .iter()
            .filter_map(|window| window.time_until_animation_frame(now))
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
            window.publish_due_animation_frame(now);
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
