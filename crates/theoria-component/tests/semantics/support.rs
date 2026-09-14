use std::{cell::RefCell, rc::Rc};

use theoria_component::{ComponentInstance, ComponentSink, MountContext, UpdateContext};
use theoria_event::{DispatchQueue, DynDispatch, EventSink, SubmissionOutcome};
use theoria_reactive::ReactiveError;

pub(super) struct TestDriver {
    queue: DispatchQueue,
    instance: ComponentInstance,
}

impl TestDriver {
    pub(super) fn new(build: impl FnOnce(&mut MountContext<'_>)) -> Self {
        let queue = DispatchQueue::new();
        let instance = ComponentInstance::new(queue.dispatcher(), build);
        Self { queue, instance }
    }

    pub(super) fn send<E, S>(
        &self,
        sink: S,
        event: E,
    ) -> Result<SubmissionOutcome, DynDispatch<'static>>
    where
        S: EventSink<E>,
        S::Dispatch: 'static,
    {
        self.instance.send(sink, event)
    }

    fn drain(&mut self, maximum: Option<usize>) -> Result<DrainOutcome, ReactiveError> {
        let mut deliveries = 0;
        loop {
            if self.instance.is_finished() {
                return Ok(DrainOutcome::Finished);
            }
            if maximum.is_some_and(|maximum| deliveries >= maximum) {
                self.instance.flush_effects()?;
                return Ok(DrainOutcome::Paused);
            }
            if self.queue.run_next().is_none() {
                self.instance.flush_effects()?;
                return Ok(DrainOutcome::Idle);
            }
            deliveries += 1;
        }
    }

    pub(super) fn run_until_idle(&mut self) -> Result<DrainOutcome, ReactiveError> {
        self.drain(None)
    }

    pub(super) fn run_events(&mut self, maximum: usize) -> Result<DrainOutcome, ReactiveError> {
        self.drain(Some(maximum))
    }
}

pub(super) struct TestRuntime<E: 'static> {
    driver: TestDriver,
    input: ComponentSink<E>,
}

pub(super) trait HandlesEvent: 'static {
    type Event: 'static;

    fn handle(&mut self, event: Self::Event, context: &mut UpdateContext<'_>);
}

pub(super) fn receive<H: HandlesEvent>(
    context: &mut MountContext<'_>,
    mut handler: H,
) -> ComponentSink<H::Event> {
    context.receive(move |event, context| handler.handle(event, context))
}

impl<E: 'static> TestRuntime<E> {
    pub(super) fn new(build: impl FnOnce(&mut MountContext<'_>) -> ComponentSink<E>) -> Self {
        let output = Rc::new(RefCell::new(None));
        let mounted_output = Rc::clone(&output);
        let driver = TestDriver::new(move |context| {
            mounted_output.replace(Some(build(context)));
        });
        let input = output
            .borrow_mut()
            .take()
            .expect("root construction produces its test input");
        Self { driver, input }
    }

    pub(super) fn new_handler<F>(builder: impl FnOnce(&mut MountContext<'_>) -> F) -> Self
    where
        F: FnMut(E, &mut UpdateContext<'_>) + 'static,
    {
        Self::new(move |context| {
            let handler = builder(context);
            context.receive(handler)
        })
    }

    pub(super) const fn input(&self) -> &ComponentSink<E> {
        &self.input
    }

    pub(super) fn send(&self, event: E) -> bool {
        self.driver.send(&self.input, event).is_ok()
    }

    pub(super) fn send_to<T: 'static>(&self, sink: &ComponentSink<T>, event: T) -> bool {
        self.driver.send(sink, event).is_ok()
    }

    pub(super) fn run_until_idle(&mut self) -> DrainOutcome {
        self.driver.run_until_idle().expect("valid Effects")
    }

    pub(super) fn run_events(&mut self, maximum: usize) -> DrainOutcome {
        self.driver.run_events(maximum).expect("valid Effects")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DrainOutcome {
    Idle,
    Paused,
    Finished,
}
