use std::{
    any::Any,
    cell::{Cell, RefCell},
    rc::Rc,
};

use super::{
    ComputeResult, EventContext, ReadContext,
    effect::{EffectArena, EffectId, EffectNode},
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct NodeId {
    index: u32,
    generation: u32,
}

pub(super) struct RuntimeInner {
    nodes: RefCell<NodeArena>,
    effects: RefCell<EffectArena>,
    phase: Cell<RuntimePhase>,
}

impl RuntimeInner {
    pub(super) fn new() -> Self {
        Self {
            nodes: RefCell::new(NodeArena::new()),
            effects: RefCell::new(EffectArena::new()),
            phase: Cell::new(RuntimePhase::Idle),
        }
    }

    pub(super) fn run_event_turn<R>(&self, handler: impl FnOnce(&mut EventContext<'_>) -> R) -> R {
        let result = {
            let _phase = self.enter_phase(RuntimePhase::Event);
            let mut context = EventContext::new(self);
            handler(&mut context)
        };
        self.flush_effects();
        result
    }

    pub(super) fn flush_effects(&self) {
        let _phase = self.enter_phase(RuntimePhase::Effects);
        let scheduled = self.effects.borrow().ordered();
        for (_, id, effect) in scheduled {
            if self.effects.borrow().contains(id) {
                effect.run_if_needed(self, id);
            }
        }
    }

    pub(super) fn insert(&self, node: Rc<dyn ErasedNode>) -> NodeId {
        self.nodes.borrow_mut().insert(node)
    }

    pub(super) fn remove(&self, id: NodeId) {
        let removed = self.nodes.borrow_mut().remove(id);
        drop(removed);
    }

    pub(super) fn insert_effect(&self, effect: Rc<EffectNode>) -> EffectId {
        self.effects.borrow_mut().insert(effect)
    }

    pub(super) fn remove_effect(&self, id: EffectId) {
        let removed = self.effects.borrow_mut().remove(id);
        drop(removed);
    }

    pub(super) fn resolve<T: 'static>(&self, id: NodeId) -> Rc<T> {
        let node = self.node(id);
        let value = node.resolve_erased(self, id);
        value.downcast::<T>().unwrap_or_else(|_| {
            panic!("reactive node {id:?} was read through an incompatible Value type")
        })
    }

    pub(super) fn revision(&self, id: NodeId) -> u64 {
        self.node(id).revision()
    }

    pub(super) fn refresh(&self, id: NodeId) {
        self.node(id).refresh(self, id);
    }

    pub(super) fn contains(&self, id: NodeId) -> bool {
        self.nodes.borrow().get(id).is_some()
    }

    pub(super) fn with_state<T: 'static, R>(
        &self,
        id: NodeId,
        operation: impl FnOnce(&StateNode<T>) -> R,
    ) -> R {
        let node = self.node(id);
        let state = node
            .as_any()
            .downcast_ref::<StateNode<T>>()
            .unwrap_or_else(|| panic!("reactive node {id:?} is not writable State"));
        operation(state)
    }

    fn node(&self, id: NodeId) -> Rc<dyn ErasedNode> {
        self.nodes
            .borrow()
            .get(id)
            .unwrap_or_else(|| panic!("stale reactive handle for node {id:?}"))
    }

    fn enter_phase(&self, phase: RuntimePhase) -> RuntimePhaseGuard<'_> {
        let current = self.phase.get();
        assert_eq!(
            current,
            RuntimePhase::Idle,
            "reactive Runtime cannot enter {phase:?} while it is in {current:?}"
        );
        self.phase.set(phase);
        RuntimePhaseGuard { runtime: self }
    }

    #[cfg(test)]
    pub(super) fn node_count(&self) -> usize {
        self.nodes.borrow().len()
    }

    #[cfg(test)]
    pub(super) fn effect_count(&self) -> usize {
        self.effects.borrow().len()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RuntimePhase {
    Idle,
    Event,
    Effects,
}

struct RuntimePhaseGuard<'a> {
    runtime: &'a RuntimeInner,
}

impl Drop for RuntimePhaseGuard<'_> {
    fn drop(&mut self) {
        self.runtime.phase.set(RuntimePhase::Idle);
    }
}

struct NodeArena {
    slots: Vec<NodeSlot>,
    free: Vec<u32>,
    len: usize,
}

struct NodeSlot {
    generation: u32,
    node: Option<Rc<dyn ErasedNode>>,
}

impl NodeArena {
    fn new() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
            len: 0,
        }
    }

    fn insert(&mut self, node: Rc<dyn ErasedNode>) -> NodeId {
        let id = if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[index as usize];
            debug_assert!(slot.node.is_none());
            slot.node = Some(node);
            NodeId {
                index,
                generation: slot.generation,
            }
        } else {
            let index =
                u32::try_from(self.slots.len()).expect("reactive Runtime node capacity exceeded");
            self.slots.push(NodeSlot {
                generation: 0,
                node: Some(node),
            });
            NodeId {
                index,
                generation: 0,
            }
        };
        self.len += 1;
        id
    }

    fn remove(&mut self, id: NodeId) -> Option<Rc<dyn ErasedNode>> {
        let slot = self.slots.get_mut(id.index as usize)?;
        if slot.generation != id.generation {
            return None;
        }
        let node = slot.node.take()?;
        slot.generation = slot
            .generation
            .checked_add(1)
            .expect("reactive node generation exhausted");
        self.free.push(id.index);
        self.len -= 1;
        Some(node)
    }

    fn get(&self, id: NodeId) -> Option<Rc<dyn ErasedNode>> {
        let slot = self.slots.get(id.index as usize)?;
        if slot.generation != id.generation {
            return None;
        }
        slot.node.as_ref().map(Rc::clone)
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.len
    }
}

pub(super) trait ErasedNode {
    fn as_any(&self) -> &dyn Any;
    fn refresh(&self, runtime: &RuntimeInner, id: NodeId);
    fn resolve_erased(&self, runtime: &RuntimeInner, id: NodeId) -> Rc<dyn Any>;
    fn revision(&self) -> u64;
}

pub(super) struct StateNode<T> {
    value: RefCell<Rc<T>>,
    revision: Cell<u64>,
}

impl<T> StateNode<T> {
    pub(super) fn new(value: T) -> Self {
        Self {
            value: RefCell::new(Rc::new(value)),
            revision: Cell::new(0),
        }
    }

    pub(super) fn set(&self, value: T) {
        self.value.replace(Rc::new(value));
        advance_revision(&self.revision);
    }

    pub(super) fn update(&self, update: impl FnOnce(&T) -> T) {
        let updated = {
            let current = self.value.borrow();
            update(current.as_ref())
        };
        self.set(updated);
    }
}

impl<T: 'static> ErasedNode for StateNode<T> {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn refresh(&self, _runtime: &RuntimeInner, _id: NodeId) {}

    fn resolve_erased(&self, _runtime: &RuntimeInner, _id: NodeId) -> Rc<dyn Any> {
        self.value.borrow().clone()
    }

    fn revision(&self) -> u64 {
        self.revision.get()
    }
}

pub(super) struct ComputedNode<T> {
    builder: Box<ComputedBuilder<T>>,
    state: RefCell<ComputedState<T>>,
    revision: Cell<u64>,
    evaluating: Cell<bool>,
}

type ComputedBuilder<T> = dyn Fn(&mut ReadContext<'_>, Option<&T>) -> ComputeResult<T>;

struct ComputedState<T> {
    value: Option<Rc<T>>,
    dependencies: Vec<Dependency>,
}

impl<T> ComputedNode<T> {
    pub(super) fn new(
        builder: impl Fn(&mut ReadContext<'_>, Option<&T>) -> ComputeResult<T> + 'static,
    ) -> Self {
        Self {
            builder: Box::new(builder),
            state: RefCell::new(ComputedState {
                value: None,
                dependencies: Vec::new(),
            }),
            revision: Cell::new(0),
            evaluating: Cell::new(false),
        }
    }

    fn resolve_current(&self, runtime: &RuntimeInner, id: NodeId) -> Rc<T> {
        let _guard = EvaluationGuard::enter(&self.evaluating, id);
        let (previous, dependencies) = {
            let state = self.state.borrow();
            (state.value.clone(), state.dependencies.clone())
        };

        if let Some(previous) = &previous
            && dependencies
                .iter()
                .all(|dependency| !dependency.changed(runtime))
        {
            return Rc::clone(previous);
        }

        let mut context = ReadContext::new(runtime);
        let result = (self.builder)(&mut context, previous.as_deref());
        let value = match result {
            ComputeResult::Keep => previous
                .unwrap_or_else(|| panic!("computed node {id:?} kept a missing initial value")),
            ComputeResult::Replace(value) => {
                advance_revision(&self.revision);
                Rc::new(value)
            }
        };
        self.state.replace(ComputedState {
            value: Some(Rc::clone(&value)),
            dependencies: context.into_dependencies(),
        });
        value
    }
}

impl<T: 'static> ErasedNode for ComputedNode<T> {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn refresh(&self, runtime: &RuntimeInner, id: NodeId) {
        self.resolve_current(runtime, id);
    }

    fn resolve_erased(&self, runtime: &RuntimeInner, id: NodeId) -> Rc<dyn Any> {
        self.resolve_current(runtime, id)
    }

    fn revision(&self) -> u64 {
        self.revision.get()
    }
}

#[derive(Clone)]
pub(super) struct Dependency {
    id: NodeId,
    revision: u64,
}

impl Dependency {
    pub(super) fn new(id: NodeId, revision: u64) -> Self {
        Self { id, revision }
    }

    pub(super) fn id(&self) -> NodeId {
        self.id
    }

    pub(super) fn set_revision(&mut self, revision: u64) {
        self.revision = revision;
    }

    pub(super) fn changed(&self, runtime: &RuntimeInner) -> bool {
        if !runtime.contains(self.id) {
            return true;
        }
        runtime.refresh(self.id);
        runtime.revision(self.id) != self.revision
    }
}

struct EvaluationGuard<'a> {
    evaluating: &'a Cell<bool>,
}

impl<'a> EvaluationGuard<'a> {
    fn enter(evaluating: &'a Cell<bool>, id: NodeId) -> Self {
        assert!(
            !evaluating.replace(true),
            "reactive dependency cycle detected while evaluating node {id:?}"
        );
        Self { evaluating }
    }
}

impl Drop for EvaluationGuard<'_> {
    fn drop(&mut self) {
        self.evaluating.set(false);
    }
}

fn advance_revision(revision: &Cell<u64>) {
    let next = revision
        .get()
        .checked_add(1)
        .expect("reactive revision space exhausted");
    revision.set(next);
}
