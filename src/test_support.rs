use std::{cell::RefCell, rc::Rc};

use crate::{
    color::{ColorProfile, ColorSpace, ConnectionSpace, SpaceConnectionError, Transfer},
    component::{ComponentInstance, MountContext},
    event::{DispatchQueue, DynDispatch, EventSink, SubmissionOutcome},
    reactive::ReactiveError,
};

/// A separate runtime space with the same conversions and coordinate geometry.
/// This exercises space identity without manufacturing another standard space.
pub(crate) fn distinct_color_space(space: impl Into<ColorSpace>) -> ColorSpace {
    struct Distinct(ColorSpace);

    impl ColorProfile for Distinct {
        fn model(&self) -> crate::color::ColorModel {
            self.0.model()
        }

        fn polar_hue_index(&self) -> Option<usize> {
            self.0.polar_hue_index()
        }

        fn hue_is_powerless(&self, coordinates: &[f64]) -> bool {
            self.0.hue_is_powerless(coordinates)
        }

        fn connection_space(&self) -> ConnectionSpace {
            self.0.connection_space()
        }

        fn transfer_to_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
            self.0.transfer_to_connection_space()
        }

        fn transfer_from_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
            self.0.transfer_from_connection_space()
        }

        fn direct_transfer_to(
            &self,
            destination: &ColorSpace,
        ) -> Result<Option<Transfer>, SpaceConnectionError> {
            self.0.direct_to(destination)
        }
    }

    let space = space.into();
    ColorSpace::new(Distinct(space))
}

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

    pub(crate) fn send<E, S>(
        &self,
        sink: S,
        event: E,
    ) -> Result<SubmissionOutcome, DynDispatch<'static>>
    where
        S: EventSink<E>,
        S::Dispatch: 'static,
    {
        self.components.send(sink, event)
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
