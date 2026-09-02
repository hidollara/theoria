use std::rc::Rc;

use theoria_event::{
    DispatchReceipt, DispatchRoute, EventHandler, EventSink, EventSource, SubmissionOutcome,
    channel,
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
    pub fn event_source<E: 'static>(&mut self) -> (EventSource<E>, EventSink<E>) {
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

    /// Connects one Source to a Sink through its standard borrowed conversion.
    ///
    /// The current Component owns the connection. Returns `false` when the
    /// Source's EventProvider has already been dropped.
    pub fn connect<S, T>(&mut self, source: &EventSource<S>, sink: impl Into<EventSink<T>>) -> bool
    where
        S: 'static,
        T: 'static,
        for<'event> T: From<&'event S>,
    {
        self.connect_map(source, sink, |event| T::from(event))
    }

    /// Connects one Source to a Sink through an explicitly selected route.
    pub fn connect_via<S, T>(
        &mut self,
        route: &DispatchRoute,
        source: &EventSource<S>,
        sink: impl Into<EventSink<T>>,
    ) -> bool
    where
        S: 'static,
        T: 'static,
        for<'event> T: From<&'event S>,
    {
        self.connect_map_via(route, source, sink, |event| T::from(event))
    }

    /// Connects one Source by cloning each published value for this Sink.
    pub fn connect_cloned<S, T>(
        &mut self,
        source: &EventSource<S>,
        sink: impl Into<EventSink<T>>,
    ) -> bool
    where
        S: Clone + 'static,
        T: From<S> + 'static,
    {
        self.connect_map(source, sink, |event| T::from(event.clone()))
    }

    /// Connects one Source through an explicitly selected route, cloning each
    /// published value for this Sink.
    pub fn connect_cloned_via<S, T>(
        &mut self,
        route: &DispatchRoute,
        source: &EventSource<S>,
        sink: impl Into<EventSink<T>>,
    ) -> bool
    where
        S: Clone + 'static,
        T: From<S> + 'static,
    {
        self.connect_map_via(route, source, sink, |event| T::from(event.clone()))
    }

    /// Connects one Source using an explicit adapter into the Sink's event
    /// vocabulary.
    pub fn connect_map<S, T, F>(
        &mut self,
        source: &EventSource<S>,
        sink: impl Into<EventSink<T>>,
        map: F,
    ) -> bool
    where
        S: 'static,
        T: 'static,
        F: Fn(&S) -> T + 'static,
    {
        self.connect_map_via(self.tree.default_route(), source, sink, map)
    }

    /// Connects one Source using an explicit adapter and dispatch route.
    pub fn connect_map_via<S, T, F>(
        &mut self,
        route: &DispatchRoute,
        source: &EventSource<S>,
        sink: impl Into<EventSink<T>>,
        map: F,
    ) -> bool
    where
        S: 'static,
        T: 'static,
        F: Fn(&S) -> T + 'static,
    {
        let Some(subscription) = source.subscribe_map(sink, route, map) else {
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

    /// Sends one event to `sink` through this Component tree's default route.
    pub fn send<E: 'static>(
        &mut self,
        sink: impl Into<EventSink<E>>,
        event: E,
    ) -> Result<SubmissionOutcome, E> {
        self.tree.default_route().send(sink, event)
    }

    /// Sends one event to `sink` through an explicitly selected route.
    pub fn send_via<E: 'static>(
        &mut self,
        route: &DispatchRoute,
        sink: impl Into<EventSink<E>>,
        event: E,
    ) -> Result<SubmissionOutcome, E> {
        route.send(sink, event)
    }

    /// Sends one tracked event through this Component tree's default route.
    pub fn send_tracked<E: 'static>(
        &mut self,
        sink: impl Into<EventSink<E>>,
        event: E,
    ) -> Result<DispatchReceipt, E> {
        self.tree.default_route().send_tracked(sink, event)
    }

    /// Sends one tracked event through an explicitly selected route.
    pub fn send_tracked_via<E: 'static>(
        &mut self,
        route: &DispatchRoute,
        sink: impl Into<EventSink<E>>,
        event: E,
    ) -> Result<DispatchReceipt, E> {
        route.send_tracked(sink, event)
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
