//! Default local execution of one Component tree.

use std::{
    cell::RefCell,
    error::Error,
    time::{Duration, Instant},
};

use crate::{
    component::{ComponentInstance, MountContext},
    event::FifoDispatcher,
    event_loop::EventLoopParticipant,
    reactive::ReactiveError,
};

/// Drives one Component tree without assigning Window ownership to it.
pub(crate) struct ComponentExecution {
    state: RefCell<ComponentExecutionState>,
}

struct ComponentExecutionState {
    dispatcher: FifoDispatcher,
    instance: ComponentInstance,
}

impl ComponentExecution {
    pub(crate) fn mount<R>(build: impl FnOnce(&mut MountContext<'_>) -> R) -> (Self, R) {
        let dispatcher = FifoDispatcher::new();
        let mut result = None;
        let mut build = Some(build);
        let instance = ComponentInstance::new(dispatcher.route(), |context| {
            result = Some(build.take().expect("the Component build runs once")(
                context,
            ));
        });
        (
            Self {
                state: RefCell::new(ComponentExecutionState {
                    dispatcher,
                    instance,
                }),
            },
            result.expect("the Component build runs once"),
        )
    }

    pub(crate) fn flush_effects(&self) -> Result<(), ReactiveError> {
        self.state.borrow_mut().instance.flush_effects()
    }

    fn advance(&self, available_time: Duration) -> Result<(), ReactiveError> {
        self.state.borrow_mut().advance(available_time)
    }
}

impl EventLoopParticipant for ComponentExecution {
    fn is_closed(&self) -> bool {
        self.state.borrow().instance.is_finished()
    }

    fn keeps_alive(&self) -> bool {
        false
    }

    fn is_idle(&self) -> bool {
        !self.state.borrow().dispatcher.has_pending()
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

impl ComponentExecutionState {
    fn advance(&mut self, available_time: Duration) -> Result<(), ReactiveError> {
        let started_at = Instant::now();
        loop {
            if self.instance.is_finished() {
                return Ok(());
            }
            if !self.dispatcher.deliver_next() {
                self.instance.flush_effects()?;
                return Ok(());
            }
            if self.instance.is_finished() {
                return Ok(());
            }
            if started_at.elapsed() >= available_time {
                self.instance.flush_effects()?;
                return Ok(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use super::*;

    #[test]
    fn an_effect_that_enqueues_an_event_keeps_component_work_pending() {
        let dispatcher = FifoDispatcher::new();
        let route = dispatcher.route();
        let received = Rc::new(Cell::new(false));
        let handler_output = Rc::clone(&received);
        let effect_route = route.clone();
        let instance = ComponentInstance::new(route, |context| {
            let input = context.receive(move |(): (), _| handler_output.set(true));
            let effect = context.create_effect(move |_| {
                assert!(effect_route.send(&input, ()).is_ok());
            });
            context.sync(&effect);
        });
        let execution = ComponentExecution {
            state: RefCell::new(ComponentExecutionState {
                dispatcher,
                instance,
            }),
        };

        execution
            .advance(Duration::MAX)
            .expect("the synchronized Effect is valid");
        assert!(execution.state.borrow().dispatcher.has_pending());
        assert!(!received.get());

        execution
            .advance(Duration::MAX)
            .expect("the event handler is valid");
        assert!(!execution.state.borrow().dispatcher.has_pending());
        assert!(received.get());
    }
}
