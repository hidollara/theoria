use std::{cell::RefCell, rc::Rc};

use crate::{
    color::{
        ColorSpace, ColorSpaceId, ColorTransform, IccRenderingIntent, Pcs, ProfileConnectionError,
        ProfileDefinition,
    },
    component::{ComponentInstance, MountContext},
    event::{DispatchQueue, DynDispatch, EventSink, SubmissionOutcome},
    reactive::ReactiveError,
};

/// A separate profile definition with the same conversions and coordinate space.
/// This exercises profile identity without manufacturing another standard profile.
pub(crate) fn distinct_color_profile(profile: impl Into<ColorSpace>) -> ColorSpace {
    struct Distinct(ColorSpace);

    impl ProfileDefinition for Distinct {
        fn space(&self) -> ColorSpaceId {
            self.0.space()
        }

        fn pcs(&self) -> Pcs {
            self.0.pcs()
        }

        fn space_to_pcs(
            &self,
            intent: IccRenderingIntent,
        ) -> Result<ColorTransform, ProfileConnectionError> {
            self.0.space_to_pcs(intent)
        }

        fn pcs_to_space(
            &self,
            intent: IccRenderingIntent,
        ) -> Result<ColorTransform, ProfileConnectionError> {
            self.0.pcs_to_space(intent)
        }

        fn direct_to(
            &self,
            destination: &ColorSpace,
            intent: IccRenderingIntent,
        ) -> Result<Option<ColorTransform>, ProfileConnectionError> {
            self.0.direct_to(destination, intent)
        }
    }

    ColorSpace::new(Distinct(profile.into()))
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
