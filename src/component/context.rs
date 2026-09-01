use crate::reactive::{
    ComputeResult, EffectContext, EventContext, ReactiveScope, ReadContext, Snapshot, State, Value,
};

use super::arena::ComponentId;
use super::{
    Component, ComponentHandle, EventEmitter, EventSource, EventTarget, event,
    runtime::ComponentRuntimeInner,
};

/// Reactive construction capabilities scoped to one mounted component.
///
/// Every dynamic value created here is released together when the component is
/// unmounted. Scopes are ownership groups, not dependency boundaries: values
/// from other scopes in the same reactive Runtime can still be read.
pub struct MountContext<'mount> {
    scope: &'mount ReactiveScope,
    runtime: &'mount ComponentRuntimeInner,
    children: &'mount mut Vec<ComponentId>,
    resources: &'mount mut Vec<Box<dyn super::ComponentResource>>,
}

impl<'mount> MountContext<'mount> {
    pub(super) fn new(
        scope: &'mount ReactiveScope,
        runtime: &'mount ComponentRuntimeInner,
        children: &'mount mut Vec<ComponentId>,
        resources: &'mount mut Vec<Box<dyn super::ComponentResource>>,
    ) -> Self {
        Self {
            scope,
            runtime,
            children,
            resources,
        }
    }

    /// Creates one lifecycle-bound typed event source and its publication
    /// capability.
    ///
    /// The source remains valid until this component is unmounted. The source
    /// can be subscribed to repeatedly; cloning the emitter explicitly shares
    /// publication authority.
    pub fn event<E: 'static>(&mut self) -> (EventSource<E>, EventEmitter<E>) {
        let (source, emitter, owner) = event::event(self.runtime);
        self.resources.push(owner);
        (source, emitter)
    }

    /// Connects one source to a component through its standard borrowed event
    /// conversion.
    ///
    /// The connection is owned by the component currently being constructed.
    pub fn subscribe<C, E>(&mut self, source: &EventSource<E>, target: &ComponentHandle<C>) -> bool
    where
        C: Component,
        E: 'static,
        for<'event> C::Event: From<&'event E>,
    {
        self.subscribe_with(source, target, |event| C::Event::from(event))
    }

    /// Connects one source using an explicit adapter into the target's event
    /// vocabulary.
    ///
    /// The adapter observes the published value by reference and creates the
    /// owned event queued for this target. Event handling itself remains in
    /// [`Component::receive`].
    pub fn subscribe_with<C, E, F>(
        &mut self,
        source: &EventSource<E>,
        target: &ComponentHandle<C>,
        map: F,
    ) -> bool
    where
        C: Component,
        E: 'static,
        F: Fn(&E) -> C::Event + 'static,
    {
        let Some(subscription) = self.runtime.subscribe(source, target, map) else {
            return false;
        };
        self.resources.push(subscription);
        true
    }

    /// Mounts a component owned for the lifetime of the component currently
    /// being constructed.
    ///
    /// This ownership relation does not imply drawing order, event routing, or
    /// a coordinate-space relationship.
    pub fn mount<C: Component>(
        &mut self,
        builder: impl FnOnce(&mut MountContext<'_>) -> C,
    ) -> ComponentHandle<C> {
        let child = self.runtime.mount_component(builder);
        self.children.push(child.id());
        child
    }

    pub fn state<T: 'static>(&self, value: T) -> State<T> {
        self.scope.state(value)
    }

    pub fn computed<T: 'static>(
        &self,
        builder: impl Fn(&mut ReadContext<'_>) -> T + 'static,
    ) -> Value<T> {
        self.scope.computed(builder)
    }

    pub fn computed_incremental<T: 'static>(
        &self,
        builder: impl Fn(&mut ReadContext<'_>, Option<&T>) -> ComputeResult<T> + 'static,
    ) -> Value<T> {
        self.scope.computed_incremental(builder)
    }

    pub fn effect(&self, effect: impl FnMut(&mut EffectContext<'_>) + 'static) {
        self.scope.effect(effect);
    }
}

/// Capabilities available while a component handles one event.
pub struct ComponentContext<'turn> {
    runtime: &'turn ComponentRuntimeInner,
    reactive: EventContext<'turn>,
    current: ComponentId,
}

impl<'turn> ComponentContext<'turn> {
    pub(super) fn new(
        runtime: &'turn ComponentRuntimeInner,
        reactive: EventContext<'turn>,
        current: ComponentId,
    ) -> Self {
        Self {
            runtime,
            reactive,
            current,
        }
    }

    /// Queues a typed direct event for later FIFO delivery.
    ///
    /// This never invokes the target recursively. `false` means the target is
    /// already unmounted or has an unmount pending.
    pub fn send<C, E>(&mut self, target: &ComponentHandle<C>, event: E) -> bool
    where
        C: Component,
        E: Into<C::Event>,
    {
        self.runtime.enqueue_for(target, event.into())
    }

    /// Queues a direct event for a type-erased destination.
    ///
    /// This is the routing counterpart to [`Self::send`]: the event type stays
    /// explicit while the concrete receiving component type is hidden.
    pub fn send_to<E: 'static>(&mut self, target: &EventTarget<E>, event: E) -> bool {
        self.runtime.enqueue_target(target, event)
    }

    /// Publishes one event to every current subscriber in subscription order.
    ///
    /// Delivery is queued and never invokes a subscriber recursively. `false`
    /// means the component scope that created the source is no longer active.
    pub fn emit<E: 'static>(&mut self, emitter: &EventEmitter<E>, event: E) -> bool {
        self.runtime.emit(emitter, event)
    }

    /// Creates a source owned by the component handling this event.
    pub fn event<E: 'static>(&mut self) -> (EventSource<E>, EventEmitter<E>) {
        let (source, emitter, owner) = event::event(self.runtime);
        assert!(
            self.runtime.add_resource(self.current, owner),
            "cannot create an event source while the current component is unmounting"
        );
        (source, emitter)
    }

    /// Adds a subscription through the target's standard borrowed event
    /// conversion.
    pub fn subscribe<C, E>(&mut self, source: &EventSource<E>, target: &ComponentHandle<C>) -> bool
    where
        C: Component,
        E: 'static,
        for<'event> C::Event: From<&'event E>,
    {
        self.subscribe_with(source, target, |event| C::Event::from(event))
    }

    /// Adds a subscription with an explicit adapter into the target's event
    /// vocabulary.
    pub fn subscribe_with<C, E, F>(
        &mut self,
        source: &EventSource<E>,
        target: &ComponentHandle<C>,
        map: F,
    ) -> bool
    where
        C: Component,
        E: 'static,
        F: Fn(&E) -> C::Event + 'static,
    {
        let Some(subscription) = self.runtime.subscribe(source, target, map) else {
            return false;
        };
        self.runtime.add_resource(self.current, subscription)
    }

    /// Mounts a component owned by the component handling this event.
    ///
    /// The new component is addressable immediately, but its handlers still run
    /// only through queued event delivery and are never invoked recursively.
    pub fn mount<C: Component>(
        &mut self,
        builder: impl FnOnce(&mut MountContext<'_>) -> C,
    ) -> ComponentHandle<C> {
        self.runtime.mount_child(self.current, builder)
    }

    /// Requests removal of a mounted component.
    ///
    /// If the target is currently handling an event, destruction is deferred
    /// until that handler returns.
    pub fn unmount<C: Component>(&mut self, target: &ComponentHandle<C>) -> bool {
        self.runtime.unmount_handle(target)
    }

    /// Requests removal of the component currently handling this event.
    pub fn unmount_current(&mut self) {
        let removed = self.runtime.request_unmount(self.current);
        debug_assert!(
            removed,
            "the currently dispatched component must be mounted"
        );
    }

    /// Reads the current snapshot without adding a reactive dependency.
    pub fn read_current<T: 'static>(&self, value: &Value<T>) -> Snapshot<T> {
        self.reactive.read_current(value)
    }

    /// Replaces State as part of this event turn.
    pub fn set<T: 'static>(&mut self, state: &State<T>, value: T) {
        self.reactive.set(state, value);
    }

    /// Updates State from its current value as part of this event turn.
    pub fn update<T: 'static>(&mut self, state: &State<T>, update: impl FnOnce(&T) -> T) {
        self.reactive.update(state, update);
    }
}
