use std::{cell::RefCell, rc::Rc};

use crate::{
    component::{ComponentInstance, MountContext},
    event::{EventSink, FifoDispatcher, SubmissionOutcome},
    reactive::ReactiveError,
};

pub(crate) struct TestDriver {
    dispatcher: FifoDispatcher,
    components: ComponentInstance,
}

impl TestDriver {
    fn new(build: impl FnOnce(&mut MountContext<'_>)) -> Self {
        let dispatcher = FifoDispatcher::new();
        let components = ComponentInstance::new(dispatcher.route(), build);
        Self {
            dispatcher,
            components,
        }
    }

    pub(crate) fn send<E: 'static>(
        &self,
        sink: impl Into<EventSink<E>>,
        event: E,
    ) -> Result<SubmissionOutcome, E> {
        self.components.send(sink, event)
    }

    pub(crate) fn run_until_idle(&mut self) -> Result<(), ReactiveError> {
        while self.dispatcher.deliver_next() {}
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
