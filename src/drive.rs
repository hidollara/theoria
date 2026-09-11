//! Shared scheduling policy for advancing UI work.

use std::time::{Duration, Instant};

use crate::{component::ComponentInstance, event::FifoDispatcher, reactive::ReactiveError};

pub(crate) const COMPONENT_WORK_SLICE: Duration = Duration::from_millis(4);
pub(crate) const PRESENTATION_WAIT_SLICE: Duration = Duration::from_millis(1);

pub(crate) enum ComponentProgress {
    Idle,
    Pending,
    Finished,
}

pub(crate) fn drive_components(
    dispatcher: &mut FifoDispatcher,
    components: &mut ComponentInstance,
    available_time: Duration,
) -> Result<ComponentProgress, ReactiveError> {
    let started_at = Instant::now();
    loop {
        if components.is_finished() {
            return Ok(ComponentProgress::Finished);
        }
        if !dispatcher.deliver_next() {
            components.flush_effects()?;
            return Ok(ComponentProgress::Idle);
        }
        if components.is_finished() {
            return Ok(ComponentProgress::Finished);
        }
        if started_at.elapsed() >= available_time {
            components.flush_effects()?;
            return Ok(ComponentProgress::Pending);
        }
    }
}
