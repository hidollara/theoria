//! Adaptation of local Component execution to EventLoop scheduling.

use std::{
    cell::RefCell,
    error::Error,
    time::{Duration, Instant},
};

use crate::{component::MountContext, component_execution::ComponentExecution};

use super::EventLoopParticipant;

/// Applies one EventLoop's delivery and Effect-flush policy to a Component.
pub(crate) struct ComponentParticipant {
    execution: RefCell<ComponentExecution>,
}

impl ComponentParticipant {
    pub(crate) fn mount<R>(build: impl FnOnce(&mut MountContext<'_>) -> R) -> (Self, R) {
        let (execution, result) = ComponentExecution::mount(build);
        (
            Self {
                execution: RefCell::new(execution),
            },
            result,
        )
    }

    pub(crate) fn flush_effects(&self) -> Result<(), crate::reactive::ReactiveError> {
        self.execution.borrow_mut().flush_effects()
    }

    fn advance(&self, available_time: Duration) -> Result<(), crate::reactive::ReactiveError> {
        let mut execution = self.execution.borrow_mut();
        let started_at = Instant::now();
        loop {
            if execution.is_finished() {
                return Ok(());
            }
            if !execution.deliver_next() {
                execution.flush_effects()?;
                return Ok(());
            }
            if execution.is_finished() {
                return Ok(());
            }
            if started_at.elapsed() >= available_time {
                execution.flush_effects()?;
                return Ok(());
            }
        }
    }
}

impl EventLoopParticipant for ComponentParticipant {
    fn is_closed(&self) -> bool {
        self.execution.borrow().is_finished()
    }

    fn keeps_alive(&self) -> bool {
        false
    }

    fn is_idle(&self) -> bool {
        !self.execution.borrow().has_pending_event()
    }

    fn process_ready_work(
        &self,
        _now: Instant,
        available_time: Duration,
    ) -> Result<(), Box<dyn Error>> {
        self.advance(available_time)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use crate::event::{EventTarget, OwningSink};

    use super::*;

    #[test]
    fn an_effect_that_enqueues_an_event_keeps_component_work_pending() {
        let received = Rc::new(Cell::new(false));
        let handler_output = Rc::clone(&received);
        let (participant, ()) = ComponentParticipant::mount(|context| {
            let (source, source_input) = context.event_source::<()>();
            let handler = context.receive(move |(): (), _| handler_output.set(true));
            assert!(context.connect(source, OwningSink::new(handler)));
            let effect = context.create_effect(move |_| {
                assert!(EventTarget::from(&source_input).send_inline(()).is_ok());
            });
            context.sync(&effect);
        });

        participant
            .advance(Duration::MAX)
            .expect("the synchronized Effect is valid");
        assert!(participant.execution.borrow().has_pending_event());
        assert!(!received.get());

        participant
            .advance(Duration::MAX)
            .expect("the event handler is valid");
        assert!(!participant.execution.borrow().has_pending_event());
        assert!(received.get());
    }
}
