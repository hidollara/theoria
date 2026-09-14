use std::{fmt, rc::Rc};

use theoria_event::{
    Dispatch, DispatchOutcome, DispatchReceipt, Dispatcher, EventDispatch, EventDispatcher,
    EventSink, Sink, SubmissionOutcome,
};
use theoria_reactive::{Reactive, ReactiveError};

use crate::{component::MountContext, tree::ComponentTree};

/// One live, mounted Component tree with a default event Dispatcher.
///
/// An Instance owns its Component tree, Reactive evaluation context, and Effect
/// scheduling state, but does not own or drive the underlying Dispatcher.
/// Callers decide when to execute deferred Dispatches and when to flush Effects.
/// The default Dispatcher serves ordinary sends and connections; the `*_via`
/// methods may select another Dispatcher, while `*_inline` methods explicitly
/// execute on the current call stack.
pub struct ComponentInstance {
    tree: Rc<ComponentTree>,
}

impl ComponentInstance {
    /// Mounts one root Component tree using `default_dispatcher` for ordinary
    /// Component event sends and connections.
    pub fn new(
        default_dispatcher: impl AsRef<Dispatcher>,
        build: impl FnOnce(&mut MountContext<'_>),
    ) -> Self {
        let tree = ComponentTree::new(
            Reactive::new(),
            default_dispatcher.as_ref().clone(),
            |tree, root| {
                let mut context = MountContext::new(tree, root);
                build(&mut context);
            },
        );
        Self { tree }
    }

    /// Sends one event through this Instance's default Dispatcher.
    pub fn send<E, S>(&self, sink: S, event: E) -> Result<SubmissionOutcome, S::Dispatch>
    where
        S: Sink<E>,
        S::Dispatch: 'static,
    {
        self.tree.default_dispatcher().submit(sink.send(event))
    }

    /// Sends one event directly on the current call stack.
    ///
    /// This bypasses the default Dispatcher and may synchronously enter the
    /// target Component.
    pub fn send_inline<E, S>(&self, sink: S, event: E) -> DispatchOutcome
    where
        S: Sink<E>,
    {
        sink.send(event).run()
    }

    /// Sends one event through an explicitly selected Dispatcher.
    pub fn send_via<E, S, D>(
        &self,
        dispatcher: &D,
        sink: S,
        event: E,
    ) -> Result<SubmissionOutcome, S::Dispatch>
    where
        S: Sink<E>,
        D: EventDispatcher<S::Dispatch>,
    {
        dispatcher.submit(sink.send(event))
    }

    /// Sends one tracked event through this Instance's default Dispatcher.
    pub fn send_tracked<E: 'static>(
        &self,
        sink: impl Into<EventSink<E>>,
        event: E,
    ) -> Result<DispatchReceipt, EventDispatch<'static>> {
        let (dispatch, receipt) = sink.into().send(event).tracked();
        self.tree
            .default_dispatcher()
            .submit(dispatch)
            .map(|_| receipt)
    }

    /// Sends one tracked event through an explicitly selected Dispatcher.
    pub fn send_tracked_via<E: 'static>(
        &self,
        dispatcher: impl AsRef<Dispatcher>,
        sink: impl Into<EventSink<E>>,
        event: E,
    ) -> Result<DispatchReceipt, EventDispatch<'static>> {
        let (dispatch, receipt) = sink.into().send(event).tracked();
        dispatcher.as_ref().submit(dispatch).map(|_| receipt)
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
