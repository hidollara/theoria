use std::{cell::RefCell, rc::Rc};

use crate::{
    component::{ComponentInstance, MountContext},
    event::{DispatchQueue, DynDispatch, EventTarget, SubmissionOutcome},
    reactive::ReactiveError,
};

pub(crate) struct TestDriver {
    queue: DispatchQueue,
    components: ComponentInstance,
}

impl TestDriver {
    fn new(build: impl FnOnce(&mut MountContext<'_>)) -> Self {
        let queue = DispatchQueue::new();
        let components = ComponentInstance::new(queue.dispatcher(), build);
        Self { queue, components }
    }

    pub(crate) fn send<E: 'static>(
        &self,
        sink: impl Into<EventTarget<E>>,
        event: E,
    ) -> Result<SubmissionOutcome, DynDispatch<'static>> {
        self.components.send(sink.into(), event)
    }

    pub(crate) fn run_until_idle(&mut self) -> Result<(), ReactiveError> {
        while self.queue.run_next().is_some() {}
        self.components.flush_effects()
    }
}

pub(crate) struct TestRuntime<I> {
    driver: TestDriver,
    interface: I,
}

impl<I: 'static> TestRuntime<I> {
    pub(crate) fn new(build: impl FnOnce(&mut MountContext<'_>) -> I + 'static) -> Self {
        let interface = Rc::new(RefCell::new(None));
        let mounted_interface = Rc::clone(&interface);
        let driver = TestDriver::new(move |context| {
            mounted_interface.replace(Some(build(context)));
        });
        let interface = interface
            .borrow_mut()
            .take()
            .expect("test Component construction produces its interface");
        Self { driver, interface }
    }

    pub(crate) const fn interface(&self) -> &I {
        &self.interface
    }

    pub(crate) fn into_driver(self) -> TestDriver {
        self.driver
    }
}
