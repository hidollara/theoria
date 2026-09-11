//! Window-loop scheduling of Component events and Effects.

use std::time::{Duration, Instant};

use crate::{component::ComponentInstance, event::FifoDispatcher, reactive::ReactiveError};

pub(crate) const COMPONENT_WORK_SLICE: Duration = Duration::from_millis(4);

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
            return Ok(if dispatcher.has_pending() {
                ComponentProgress::Pending
            } else {
                ComponentProgress::Idle
            });
        }
        if components.is_finished() {
            return Ok(ComponentProgress::Finished);
        }
        if started_at.elapsed() >= available_time {
            components.flush_effects()?;
            return Ok(if dispatcher.has_pending() {
                ComponentProgress::Pending
            } else {
                ComponentProgress::Idle
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use super::*;

    #[test]
    fn an_effect_that_enqueues_an_event_keeps_component_work_pending() {
        let mut dispatcher = FifoDispatcher::new();
        let route = dispatcher.route();
        let received = Rc::new(Cell::new(false));
        let handler_output = Rc::clone(&received);
        let effect_route = route.clone();
        let mut components = ComponentInstance::new(route, |context| {
            let input = context.receive(move |(): (), _| handler_output.set(true));
            let effect = context.create_effect(move |_| {
                assert!(effect_route.send(&input, ()).is_ok());
            });
            context.sync(&effect);
        });

        let progress = drive_components(&mut dispatcher, &mut components, Duration::MAX)
            .expect("the synchronized Effect is valid");
        assert!(matches!(progress, ComponentProgress::Pending));
        assert!(dispatcher.has_pending());
        assert!(!received.get());

        let progress = drive_components(&mut dispatcher, &mut components, Duration::MAX)
            .expect("the event handler is valid");
        assert!(matches!(progress, ComponentProgress::Idle));
        assert!(received.get());
    }
}
