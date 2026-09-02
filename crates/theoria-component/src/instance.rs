use std::{fmt, rc::Rc};

use theoria_event::{DispatchReceipt, DispatchRoute, EventSink, SubmissionOutcome};
use theoria_reactive::{Reactive, ReactiveError};

use crate::{component::MountContext, tree::ComponentTree};

/// One live, mounted Component tree with a default event dispatch route.
///
/// An Instance owns its Component tree, Reactive evaluation context, and Effect
/// scheduling state, but does not own or drive the Dispatcher behind its route.
/// Callers decide when to execute deferred Dispatches and when to flush Effects.
/// The default route serves ordinary sends and connections; the `*_via` methods
/// may select other routes without dividing the Component tree or its Reactive
/// context.
pub struct ComponentInstance {
    tree: Rc<ComponentTree>,
}

impl ComponentInstance {
    /// Mounts one root Component tree using `default_route` for ordinary
    /// Component event sends and connections.
    pub fn new(default_route: DispatchRoute, build: impl FnOnce(&mut MountContext<'_>)) -> Self {
        let tree = ComponentTree::new(Reactive::new(), default_route, |tree, root| {
            let mut context = MountContext::new(tree, root);
            build(&mut context);
        });
        Self { tree }
    }

    /// Sends one event through this Instance's default dispatch route.
    pub fn send<E: 'static>(
        &self,
        sink: impl Into<EventSink<E>>,
        event: E,
    ) -> Result<SubmissionOutcome, E> {
        self.tree.default_route().send(sink, event)
    }

    /// Sends one event through an explicitly selected dispatch route.
    pub fn send_via<E: 'static>(
        &self,
        route: &DispatchRoute,
        sink: impl Into<EventSink<E>>,
        event: E,
    ) -> Result<SubmissionOutcome, E> {
        route.send(sink, event)
    }

    /// Sends one tracked event through this Instance's default dispatch route.
    pub fn send_tracked<E: 'static>(
        &self,
        sink: impl Into<EventSink<E>>,
        event: E,
    ) -> Result<DispatchReceipt, E> {
        self.tree.default_route().send_tracked(sink, event)
    }

    /// Sends one tracked event through an explicitly selected dispatch route.
    pub fn send_tracked_via<E: 'static>(
        &self,
        route: &DispatchRoute,
        sink: impl Into<EventSink<E>>,
        event: E,
    ) -> Result<DispatchReceipt, E> {
        route.send_tracked(sink, event)
    }

    /// Runs pending and stale Effects at a caller-selected synchronization boundary.
    ///
    /// Each synchronized Effect that has not run or has become stale receives
    /// at most one pending request. Explicit requests made by Components remain
    /// distinct. Every pending request is then drained.
    pub fn flush_effects(&mut self) -> Result<(), ReactiveError> {
        self.tree.flush_effects()
    }

    /// Returns whether the root Component has unmounted.
    pub fn is_finished(&self) -> bool {
        !self.tree.has_root()
    }
}

impl fmt::Debug for ComponentInstance {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ComponentInstance")
            .finish_non_exhaustive()
    }
}
