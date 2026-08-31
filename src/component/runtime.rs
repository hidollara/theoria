use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::VecDeque,
    fmt,
    num::NonZeroUsize,
    rc::{Rc, Weak},
};

use crate::reactive::{EventContext, ReactiveRuntime};

use super::arena::{ComponentArena, ComponentId, ErasedComponent, TypedComponent};
use super::{Component, ComponentContext, ComponentHandle, MountContext};

/// Owns mounted components, their reactive scopes, and a direct event queue.
///
/// The Runtime is deliberately single-threaded. Event handlers run in FIFO
/// order and never recursively invoke another component.
#[derive(Clone)]
pub struct ComponentRuntime {
    inner: Rc<ComponentRuntimeInner>,
}

impl ComponentRuntime {
    pub fn new() -> Self {
        Self {
            inner: Rc::new_cyclic(|runtime| ComponentRuntimeInner::new(Weak::clone(runtime))),
        }
    }

    /// Constructs and registers a component with its own reactive ownership scope.
    pub fn mount<C: Component>(
        &self,
        builder: impl FnOnce(&mut MountContext<'_>) -> C,
    ) -> ComponentHandle<C> {
        self.inner.assert_not_constructing("mount a root component");
        self.inner.mount_component(builder)
    }

    /// Queues a typed direct event. Returns `false` for a stale target.
    pub fn enqueue<C, E>(&self, target: &ComponentHandle<C>, event: E) -> bool
    where
        C: Component,
        E: Into<C::Event>,
    {
        self.inner.assert_not_constructing("enqueue an event");
        self.inner.enqueue_for(target, event.into())
    }

    /// Explicitly removes a component and all reactive nodes it owns.
    pub fn unmount<C: Component>(&self, target: &ComponentHandle<C>) -> bool {
        self.inner.assert_not_constructing("unmount a component");
        self.inner.unmount_handle(target)
    }

    /// Processes at most `budget` queued events, then flushes reactive Effects.
    ///
    /// The Runtime only stops between handlers. Events left in the queue remain
    /// available to the next call.
    #[must_use]
    pub fn run_turn(&self, budget: TurnBudget) -> TurnOutcome {
        let _turn = TurnGuard::enter(&self.inner);
        let mut processed = 0;
        self.inner.reactive.run_event_turn(|reactive| {
            while processed < budget.max_events.get() {
                let Some(event) = self.inner.queue.borrow_mut().pop_front() else {
                    break;
                };
                processed += 1;
                self.inner.dispatch(event, reactive);
            }
        });
        self.inner.finish_pending_unmounts();

        if self.inner.queue.borrow().is_empty() {
            TurnOutcome::Idle
        } else {
            TurnOutcome::Pending
        }
    }

    /// Runs initial or externally invalidated Effects without delivering events.
    pub fn flush_effects(&self) {
        self.inner.assert_not_constructing("flush Effects");
        self.inner.reactive.flush_effects();
    }
}

impl Default for ComponentRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for ComponentRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ComponentRuntime")
            .finish_non_exhaustive()
    }
}

/// A deterministic upper bound on events removed from the queue in one turn.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TurnBudget {
    max_events: NonZeroUsize,
}

impl TurnBudget {
    pub fn events(max_events: usize) -> Self {
        Self {
            max_events: NonZeroUsize::new(max_events)
                .expect("a component turn budget must allow at least one event"),
        }
    }

    pub fn unlimited() -> Self {
        Self::events(usize::MAX)
    }
}

/// Whether a component Runtime can wait for an external wake-up.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TurnOutcome {
    /// No queued component events remain.
    Idle,
    /// Work remains and another turn should be scheduled promptly.
    Pending,
}

pub(super) struct ComponentRuntimeInner {
    self_weak: Weak<ComponentRuntimeInner>,
    reactive: ReactiveRuntime,
    components: RefCell<ComponentArena>,
    queue: RefCell<VecDeque<QueuedEvent>>,
    turn_running: Cell<bool>,
    construction_depth: Cell<usize>,
}

impl ComponentRuntimeInner {
    fn new(self_weak: Weak<ComponentRuntimeInner>) -> Self {
        Self {
            self_weak,
            reactive: ReactiveRuntime::new(),
            components: RefCell::new(ComponentArena::new()),
            queue: RefCell::new(VecDeque::new()),
            turn_running: Cell::new(false),
            construction_depth: Cell::new(0),
        }
    }

    pub(super) fn mount_component<C: Component>(
        &self,
        builder: impl FnOnce(&mut MountContext<'_>) -> C,
    ) -> ComponentHandle<C> {
        let _construction = ConstructionGuard::enter(&self.construction_depth);
        let scope = self.reactive.scope();
        let mut children = MountChildrenGuard::new(self);
        let component = {
            let mut context = MountContext::new(&scope, self, children.ids_mut());
            builder(&mut context)
        };
        let id = self
            .components
            .borrow_mut()
            .insert(Box::new(TypedComponent(component)), scope);
        self.components.borrow_mut().adopt_many(id, children.ids());
        children.commit();
        ComponentHandle::new(Weak::clone(&self.self_weak), id)
    }

    pub(super) fn mount_child<C: Component>(
        &self,
        owner: ComponentId,
        builder: impl FnOnce(&mut MountContext<'_>) -> C,
    ) -> ComponentHandle<C> {
        assert!(
            self.components.borrow().can_own_children(owner),
            "cannot mount a child beneath an unmounted component"
        );
        let child = self.mount_component(builder);
        if !self.components.borrow_mut().adopt(owner, child.id()) {
            self.rollback_mounts(&[child.id()]);
            panic!("component owner became unavailable while mounting its child");
        }
        child
    }

    pub(super) fn enqueue_for<C: Component>(
        &self,
        target: &ComponentHandle<C>,
        event: C::Event,
    ) -> bool {
        self.assert_handle_runtime(target);
        let target = target.id();
        if !self.components.borrow().accepts_events(target) {
            return false;
        }
        self.queue.borrow_mut().push_back(QueuedEvent {
            target,
            event: Box::new(event),
        });
        true
    }

    pub(super) fn unmount_handle<C: Component>(&self, target: &ComponentHandle<C>) -> bool {
        self.assert_handle_runtime(target);
        self.request_unmount(target.id())
    }

    pub(super) fn request_unmount(&self, id: ComponentId) -> bool {
        let result = self
            .components
            .borrow_mut()
            .request_unmount(id, self.turn_running.get());
        let was_mounted = result.was_mounted();
        result.dispose();
        was_mounted
    }

    fn dispatch(&self, event: QueuedEvent, reactive: &mut EventContext<'_>) {
        let Some(component) = self.components.borrow_mut().take_for_dispatch(event.target) else {
            return;
        };
        let _pending_unmounts = PendingUnmountGuard { runtime: self };
        let mut dispatch = DispatchGuard::new(self, event.target, component);
        let mut context = ComponentContext::new(self, reactive.reborrow(), event.target);
        dispatch.receive(event.event, &mut context);
    }

    fn finish_dispatch(&self, id: ComponentId, component: Box<dyn ErasedComponent>) {
        let removed = self.components.borrow_mut().finish_dispatch(id, component);
        for removed in removed {
            removed.dispose();
        }
    }

    fn assert_handle_runtime<C: Component>(&self, handle: &ComponentHandle<C>) {
        let runtime = handle
            .runtime()
            .expect("component handle used after its Runtime was dropped");
        assert!(
            std::ptr::eq(runtime.as_ref(), self),
            "component handle belongs to a different ComponentRuntime"
        );
    }

    fn assert_not_constructing(&self, operation: &str) {
        assert_eq!(
            self.construction_depth.get(),
            0,
            "cannot {operation} while a component builder is running; use MountContext capabilities"
        );
    }

    fn finish_pending_unmounts(&self) {
        let removed = self.components.borrow_mut().remove_pending();
        for component in removed {
            component.dispose();
        }
    }

    fn rollback_mounts(&self, ids: &[ComponentId]) {
        let removed = self.components.borrow_mut().remove_roots(ids);
        for component in removed {
            component.dispose();
        }
    }
}

struct QueuedEvent {
    target: ComponentId,
    event: Box<dyn Any>,
}

struct MountChildrenGuard<'runtime> {
    runtime: &'runtime ComponentRuntimeInner,
    ids: Vec<ComponentId>,
    committed: bool,
}

impl<'runtime> MountChildrenGuard<'runtime> {
    fn new(runtime: &'runtime ComponentRuntimeInner) -> Self {
        Self {
            runtime,
            ids: Vec::new(),
            committed: false,
        }
    }

    fn ids(&self) -> &[ComponentId] {
        &self.ids
    }

    fn ids_mut(&mut self) -> &mut Vec<ComponentId> {
        &mut self.ids
    }

    fn commit(mut self) {
        self.committed = true;
    }
}

impl Drop for MountChildrenGuard<'_> {
    fn drop(&mut self) {
        if !self.committed {
            self.runtime.rollback_mounts(&self.ids);
        }
    }
}

struct DispatchGuard<'runtime> {
    runtime: &'runtime ComponentRuntimeInner,
    id: ComponentId,
    component: Option<Box<dyn ErasedComponent>>,
}

impl<'runtime> DispatchGuard<'runtime> {
    fn new(
        runtime: &'runtime ComponentRuntimeInner,
        id: ComponentId,
        component: Box<dyn ErasedComponent>,
    ) -> Self {
        Self {
            runtime,
            id,
            component: Some(component),
        }
    }

    fn receive(&mut self, event: Box<dyn Any>, context: &mut ComponentContext<'_>) {
        self.component
            .as_mut()
            .expect("dispatch guard must own its component")
            .receive_erased(event, context);
    }
}

impl Drop for DispatchGuard<'_> {
    fn drop(&mut self) {
        if let Some(component) = self.component.take() {
            self.runtime.finish_dispatch(self.id, component);
        }
    }
}

struct TurnGuard<'runtime> {
    runtime: &'runtime ComponentRuntimeInner,
}

impl<'runtime> TurnGuard<'runtime> {
    fn enter(runtime: &'runtime ComponentRuntimeInner) -> Self {
        runtime.assert_not_constructing("run an event turn");
        assert!(
            !runtime.turn_running.replace(true),
            "component Runtime cannot start a nested event turn"
        );
        Self { runtime }
    }
}

impl Drop for TurnGuard<'_> {
    fn drop(&mut self) {
        self.runtime.turn_running.set(false);
        self.runtime.finish_pending_unmounts();
    }
}

struct ConstructionGuard<'runtime> {
    depth: &'runtime Cell<usize>,
}

impl<'runtime> ConstructionGuard<'runtime> {
    fn enter(depth: &'runtime Cell<usize>) -> Self {
        depth.set(
            depth
                .get()
                .checked_add(1)
                .expect("component construction depth exhausted"),
        );
        Self { depth }
    }
}

impl Drop for ConstructionGuard<'_> {
    fn drop(&mut self) {
        self.depth.set(self.depth.get() - 1);
    }
}

struct PendingUnmountGuard<'runtime> {
    runtime: &'runtime ComponentRuntimeInner,
}

impl Drop for PendingUnmountGuard<'_> {
    fn drop(&mut self) {
        self.runtime.finish_pending_unmounts();
    }
}
