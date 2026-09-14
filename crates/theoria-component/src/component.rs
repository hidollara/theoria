use std::rc::Rc;

use theoria_event::{
    Channel, ChannelSink, ChannelSource, Dispatch, DispatchHandle, DispatchOutcome, Dispatcher,
    DynDispatch, EventSink, EventSource, SubmissionOutcome, Subscription, TrackedDispatch,
};
use theoria_reactive::{
    Computed, Effect, ReadContext, SnapshotState, State, StateSetter, WriteContext,
};

use crate::{
    node::{ConstructingComponent, MountedComponent},
    sink::{ComponentReceiver, ComponentSink},
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

    /// Creates a typed [`EventSource`] and its input [`ChannelSink`].
    ///
    /// The current Component owns the Channel. The returned Source and
    /// input sink do not keep that Component alive. After it is unmounted, the
    /// Source rejects new subscriptions and input deliveries produce no events.
    pub fn event_source<E: 'static>(&mut self) -> (ChannelSource<E>, ChannelSink<E>) {
        let channel = Channel::new();
        let source = channel.source();
        let input = channel.sink();
        self.current.retain(channel);
        (source, input)
    }

    /// Retains one typed event handler in the current Component boundary.
    ///
    /// The returned sink is a non-owning input capability. Dropping the
    /// Component boundary closes the handler without extending its lifetime.
    /// During construction, a Dispatch can reach the sink but does not invoke
    /// `handler` until the complete Component tree has mounted.
    pub fn receive<E, F>(&mut self, handler: F) -> ComponentSink<E>
    where
        E: 'static,
        F: FnMut(E, &mut UpdateContext<'_>) + 'static,
    {
        let tree = Rc::downgrade(self.tree);
        let owner = self.current.weak();
        let mut handler = handler;
        let receiver = ComponentReceiver::new(move |event: E| {
            if let (Some(tree), Some(owner)) = (tree.upgrade(), owner.upgrade()) {
                tree.update(owner, |mutation, owner| {
                    let mut context = UpdateContext::new(&tree, mutation, owner);
                    handler(event, &mut context);
                });
            }
        });
        let sink = receiver.sink();
        self.current.retain(receiver);
        sink
    }

    /// Connects one event source to a sink through the tree's default Dispatcher.
    ///
    /// The sink must turn every publication into a self-contained Dispatch
    /// accepted by the default Dispatcher. The current Component owns the
    /// resulting Subscription. Returns `false` when the Source is no longer
    /// available.
    pub fn connect<S, K>(&mut self, source: S, sink: K) -> bool
    where
        S: EventSource,
        K: 'static,
        for<'sink, 'event> &'sink K: EventSink<S::Event<'event>>,
        for<'sink, 'event> <&'sink K as EventSink<S::Event<'event>>>::Dispatch: 'static,
    {
        let Some(subscription) = self.tree.default_dispatcher().connect(source, sink) else {
            return false;
        };
        self.current.retain(subscription);
        true
    }

    /// Connects one Source through an explicitly selected Dispatcher.
    pub fn connect_via<S, K, D>(&mut self, dispatcher: D, source: S, sink: K) -> bool
    where
        S: EventSource,
        K: 'static,
        D: 'static,
        for<'sink, 'event> &'sink K: EventSink<S::Event<'event>>,
        for<'sink, 'event> D: Dispatcher<<&'sink K as EventSink<S::Event<'event>>>::Dispatch>,
    {
        let Some(subscription) = Subscription::builder(source, sink).via(dispatcher) else {
            return false;
        };
        self.current.retain(subscription);
        true
    }

    /// Connects one event source to a sink on the publishing call stack.
    ///
    /// Inline connections may borrow an event whose lifetime is shorter than
    /// the Component and may synchronously enter the target Component.
    pub fn connect_inline<S, K>(&mut self, source: S, sink: K) -> bool
    where
        S: EventSource,
        K: 'static,
        for<'sink, 'event> &'sink K: EventSink<S::Event<'event>>,
    {
        let Some(subscription) = Subscription::builder(source, sink).inline() else {
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
    ///
    /// Rejection returns the still-executable, type-erased Dispatch. Use
    /// [`Self::send_via`] when rejection must preserve its concrete type.
    pub fn send<E, S>(
        &mut self,
        sink: S,
        event: E,
    ) -> Result<SubmissionOutcome, DynDispatch<'static>>
    where
        S: EventSink<E>,
        S::Dispatch: 'static,
    {
        self.tree.default_dispatcher().submit(sink.send(event))
    }

    /// Sends one event directly on the current call stack.
    ///
    /// This bypasses the default Dispatcher and may synchronously enter the
    /// target Component.
    pub fn send_inline<E, S>(&mut self, sink: S, event: E) -> DispatchOutcome
    where
        S: EventSink<E>,
    {
        sink.send(event).run()
    }

    /// Sends one event to `sink` through an explicitly selected Dispatcher.
    pub fn send_via<E, S, D>(
        &mut self,
        dispatcher: &D,
        sink: S,
        event: E,
    ) -> Result<SubmissionOutcome, S::Dispatch>
    where
        S: EventSink<E>,
        D: Dispatcher<S::Dispatch>,
    {
        dispatcher.submit(sink.send(event))
    }

    /// Sends one tracked event through this Component tree's default Dispatcher.
    ///
    /// Rejection returns the still-executable, type-erased tracked Dispatch.
    pub fn send_tracked<E, S>(
        &mut self,
        sink: S,
        event: E,
    ) -> Result<DispatchHandle, DynDispatch<'static>>
    where
        S: EventSink<E>,
        S::Dispatch: 'static,
    {
        let dispatch = sink.send(event).tracked();
        let handle = dispatch.handle();
        self.tree
            .default_dispatcher()
            .submit(dispatch)
            .map(|_| handle)
    }

    /// Sends one tracked event through an explicitly selected Dispatcher.
    pub fn send_tracked_via<E, S, D>(
        &mut self,
        dispatcher: &D,
        sink: S,
        event: E,
    ) -> Result<DispatchHandle, TrackedDispatch<S::Dispatch>>
    where
        S: EventSink<E>,
        D: Dispatcher<TrackedDispatch<S::Dispatch>>,
    {
        let dispatch = sink.send(event).tracked();
        let handle = dispatch.handle();
        dispatcher.submit(dispatch).map(|_| handle)
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
