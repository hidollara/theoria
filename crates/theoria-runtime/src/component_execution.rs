//! Default local execution state for one Component tree.

use crate::{
    component::{ComponentInstance, MountContext},
    event::DispatchQueue,
    reactive::ReactiveError,
};

/// Owns one Component tree and its default local Dispatch queue.
///
/// This type does not decide when to deliver events or flush Effects. A loop
/// adapter supplies that policy through the primitive operations below.
pub(crate) struct ComponentExecution {
    queue: DispatchQueue,
    instance: ComponentInstance,
}

impl ComponentExecution {
    pub(crate) fn mount<R>(build: impl FnOnce(&mut MountContext<'_>) -> R) -> (Self, R) {
        let queue = DispatchQueue::new();
        let mut result = None;
        let mut build = Some(build);
        let instance = ComponentInstance::new(queue.dispatcher(), |context| {
            result = Some(build.take().expect("the Component build runs once")(
                context,
            ));
        });
        (
            Self { queue, instance },
            result.expect("the Component build runs once"),
        )
    }

    pub(crate) fn is_finished(&self) -> bool {
        self.instance.is_finished()
    }

    pub(crate) fn has_pending_event(&self) -> bool {
        !self.queue.is_empty()
    }

    pub(crate) fn run_next(&mut self) -> bool {
        self.queue.run_next().is_some()
    }

    pub(crate) fn flush_effects(&mut self) -> Result<(), ReactiveError> {
        self.instance.flush_effects()
    }
}
