use std::rc::Rc;

use theoria_event::{
    ChannelSource, DispatchReceipt, Dispatcher, EventDispatch, EventDispatcher, EventHandler,
    EventSink, EventSource, Sink, SubmissionOutcome, channel,
};
use theoria_reactive::{
    Computed, Effect, ReadContext, SnapshotState, State, StateSetter, WriteContext,
};

use crate::{
    node::{ConstructingComponent, MountedComponent},
    tree::ComponentTree,
};

/// A structural unit that constructs its resources inside one lifetime boundary.
///
/// Components are consumed when mounted. Named types implement this trait to
/// keep construction logic with their data; [`MountContext::mount_with`] is
/// available for anonymous closure-defined Components.
pub trait Component {
    fn mount(self, context: &mut MountContext<'_>);
}

/// Construction capabilities for one Component being built.
pub struct MountContext<'mount> {
    tree: &'mount Rc<ComponentTree>,
    current: &'mount ConstructingComponent,
}

impl<'mount> MountContext<'mount> {
    pub(crate) fn new(
        tree: &'mount Rc<ComponentTree>,
        current: &'mount ConstructingComponent,
    ) -> Self {
        Self { tree, current }
    }

    /// Creates a typed [`EventSource`] and its input [`EventSink`].
    ///
    /// The current Component owns the EventProvider. The returned Source and
    /// input Sink do not keep that Component alive. After it is unmounted, the
    /// Source rejects new subscriptions and input deliveries produce no events.
    pub fn event_source<E: 'static>(&mut self) -> (ChannelSource<E>, EventSink<E>) {
        let (provider, source) = channel();
        let input = EventSink::from(&provider);
        self.current.retain(provider);
        (source, input)
    }

    /// Retains one typed event handler in the current Component boundary.
    ///
    /// The returned Sink is a non-owning input capability. Dropping the
    /// Component boundary closes the handler without extending its lifetime.
    /// During construction, a Dispatch can reach the Sink but does not invoke
    /// `handler` until the complete Component tree has mounted.
    pub fn receive<E, F>(&mut self, handler: F) -> EventSink<E>
    where
        E: 'static,
        F: FnMut(E, &mut UpdateContext<'_>) + 'static,
    {
        let tree = Rc::downgrade(self.tree);
        let owner = self.current.weak();
        let mut handler = handler;
        let handler = EventHandler::new(move |event: E| {
            if let (Some(tree), Some(owner)) = (tree.upgrade(), owner.upgrade()) {
                tree.update(owner, |mutation, owner| {
                    let mut context = UpdateContext::new(&tree, mutation, owner);
                    handler(event, &mut context);
                });
            }
        });
        let sink = EventSink::from(&handler);
        self.current.retain(handler);
        sink
    }

    /// Connects one deferrable Source to a Sink through the tree's default
    /// Dispatcher.
    ///
    /// The current Component owns the resulting Subscription. Returns `false`
    /// when the Source is no longer available.
    pub fn connect<S, K>(&mut self, source: S, sink: K) -> bool
    where
        S: EventSource,
        K: 'static,
        for<'event> K: Sink<S::Event<'event>>,
        for<'event> Dispatcher: EventDispatcher<<K as Sink<S::Event<'event>>>::Dispatch>,
    {
        self.connect_via(self.tree.default_dispatcher().clone(), source, sink)
    }

    /// Connects one deferrable Source through an explicitly selected Dispatcher.
    pub fn connect_via<S, K, D>(&mut self, dispatcher: D, source: S, sink: K) -> bool
    where
        S: EventSource,
        K: 'static,
        D: 'static,
        for<'event> K: Sink<S::Event<'event>>,
        for<'event> D: EventDispatcher<<K as Sink<S::Event<'event>>>::Dispatch>,
    {
        let Some(subscription) = source.connect(sink).via(dispatcher) else {
            return false;
        };
        self.current.retain(subscription);
        true
    }

    /// Connects one Source to a Sink on the publishing call stack.
    ///
    /// Inline connections may borrow an event whose lifetime is shorter than
    /// the Component and may synchronously enter the target Component.
    pub fn connect_inline<S, K>(&mut self, source: S, sink: K) -> bool
    where
        S: EventSource,
        K: 'static,
        for<'event> K: Sink<S::Event<'event>>,
    {
        let Some(subscription) = source.connect(sink).inline() else {
            return false;
        };
        self.current.retain(subscription);
        true
    }

    /// Constructs a child Component owned for the lifetime of this Component.
    ///
    /// This ownership relation does not imply drawing order, event routing, or
    /// a coordinate-space relationship. Relationships with the child are
    /// established during construction; mounting does not export an interface.
    pub fn mount(&mut self, component: impl Component) {
        let child = ConstructingComponent::new();
        {
            let mut context = MountContext::new(self.tree, &child);
            component.mount(&mut context);
        }
        self.current.adopt(child);
    }

    /// Constructs an anonymous child Component with `build`.
    pub fn mount_with(&mut self, build: impl FnOnce(&mut MountContext<'_>)) {
        let child = ConstructingComponent::new();
        {
            let mut context = MountContext::new(self.tree, &child);
            build(&mut context);
        }
        self.current.adopt(child);
    }

    /// Retains an arbitrary resource for the current Component's lifetime.
    pub fn retain<T: 'static>(&mut self, value: T) {
        self.current.retain(value);
    }

    /// Creates State and retains it for the current Component's lifetime.
    pub fn create_state<T: 'static>(&mut self, value: T) -> State<T> {
        let state = self.tree.reactive().create_state(value);
        self.current.retain(state.clone());
        state
    }

    /// Creates snapshot-preserving State and retains it for the current
    /// Component's lifetime.
    pub fn create_snapshot_state<T: 'static>(&mut self, value: T) -> SnapshotState<T> {
        let state = self.tree.reactive().create_snapshot_state(value);
        self.current.retain(state.clone());
        state
    }

    /// Creates a lazily evaluated value and retains it for the current
    /// Component's lifetime.
    pub fn create_computed<T: 'static>(
        &mut self,
        builder: impl Fn(&mut ReadContext<'_>) -> T + 'static,
    ) -> Computed<T> {
        let computed = self.tree.reactive().create_computed(builder);
        self.current.retain(computed.clone());
        computed
    }

    /// Creates an Effect and retains it for the current Component's lifetime.
    pub fn create_effect(&mut self, effect: impl FnMut(&mut ReadContext<'_>) + 'static) -> Effect {
        let effect = self.tree.reactive().create_effect(effect);
        self.current.retain(effect.clone());
        effect
    }

    /// Schedules a stale Effect for this Component's lifetime.
    ///
    /// A newly created Effect is initially stale. After an Effect runs, each
    /// subsequent stale period creates at most one pending request. Execution
    /// occurs when [`ComponentInstance::flush_effects`](crate::ComponentInstance::flush_effects)
    /// is called.
    pub fn sync(&mut self, effect: &Effect) {
        self.tree.sync_effect(self.current, effect);
    }
}

/// Mutation and structural capabilities available while handling one event.
pub struct UpdateContext<'delivery> {
    tree: &'delivery Rc<ComponentTree>,
    mutation: WriteContext<'delivery>,
    current: MountedComponent,
}

impl<'delivery> UpdateContext<'delivery> {
    pub(crate) fn new(
        tree: &'delivery Rc<ComponentTree>,
        mutation: WriteContext<'delivery>,
        current: MountedComponent,
    ) -> Self {
        Self {
            tree,
            mutation,
            current,
        }
    }

    /// Constructs a child Component owned by the Component handling this event.
    ///
    /// Returns `false` without mounting `component` if the current Component has
    /// already requested unmounting.
    pub fn mount(&mut self, component: impl Component) -> bool {
        if !self.current.is_live() {
            return false;
        }
        let child = ConstructingComponent::new();
        {
            let mut context = MountContext::new(self.tree, &child);
            component.mount(&mut context);
        }
        self.current.mount(child)
    }

    /// Constructs an anonymous child Component while handling this event.
    ///
    /// Returns `false` without invoking `build` if the current Component has
    /// already requested unmounting.
    pub fn mount_with(&mut self, build: impl FnOnce(&mut MountContext<'_>)) -> bool {
        if !self.current.is_live() {
            return false;
        }
        let child = ConstructingComponent::new();
        {
            let mut context = MountContext::new(self.tree, &child);
            build(&mut context);
        }
        self.current.mount(child)
    }

    /// Requests removal of the Component currently handling this event.
    ///
    /// The Component and its descendants become inactive immediately. The
    /// current handler still runs to completion, after which their retained
    /// resources are released. Repeated requests are harmless, and pending
    /// Effects owned by the removed subtree are skipped.
    pub fn unmount(&mut self) {
        self.tree.request_unmount(&self.current);
    }

    /// Sends one event to `sink` through this Component tree's default Dispatcher.
    pub fn send<E: 'static>(
        &mut self,
        sink: impl Into<EventSink<E>>,
        event: E,
    ) -> Result<SubmissionOutcome, EventDispatch<'static>> {
        self.tree
            .default_dispatcher()
            .submit(sink.into().send(event))
    }

    /// Sends one event directly on the current call stack.
    ///
    /// This bypasses the default Dispatcher and may synchronously enter the
    /// target Component.
    pub fn send_inline<E>(&mut self, sink: impl Into<EventSink<E>>, event: E) -> Result<(), E> {
        sink.into().send_inline(event)
    }

    /// Sends one event to `sink` through an explicitly selected Dispatcher.
    pub fn send_via<E: 'static>(
        &mut self,
        dispatcher: impl AsRef<Dispatcher>,
        sink: impl Into<EventSink<E>>,
        event: E,
    ) -> Result<SubmissionOutcome, EventDispatch<'static>> {
        dispatcher.as_ref().submit(sink.into().send(event))
    }

    /// Sends one tracked event through this Component tree's default Dispatcher.
    pub fn send_tracked<E: 'static>(
        &mut self,
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
        &mut self,
        dispatcher: impl AsRef<Dispatcher>,
        sink: impl Into<EventSink<E>>,
        event: E,
    ) -> Result<DispatchReceipt, EventDispatch<'static>> {
        let (dispatch, receipt) = sink.into().send(event).tracked();
        dispatcher.as_ref().submit(dispatch).map(|_| receipt)
    }

    /// Replaces State or SnapshotState while handling the current event.
    pub fn set<'state, T: 'static>(&mut self, state: impl Into<StateSetter<'state, T>>, value: T) {
        self.mutation.set(state, value)
    }

    /// Mutates State in place while handling the current event.
    pub fn modify<T: 'static>(&mut self, state: &State<T>, modify: impl FnOnce(&mut T)) {
        self.mutation.modify(state, modify)
    }

    /// Replaces State and returns ownership of its previous value.
    pub fn replace<T: 'static>(&mut self, state: &State<T>, value: T) -> T {
        self.mutation.replace(state, value)
    }

    /// Replaces State or SnapshotState with an update derived from its current value.
    pub fn update<'state, T: 'static>(
        &mut self,
        state: impl Into<StateSetter<'state, T>>,
        update: impl FnOnce(&T) -> T,
    ) {
        self.mutation.update(state, update)
    }

    /// Queues one unconditional Effect execution for the next explicit Effect
    /// flush. Repeated requests remain repeated executions.
    pub fn request_effect(&mut self, effect: &Effect) {
        self.tree.request_effect(&self.current, effect);
    }
}
