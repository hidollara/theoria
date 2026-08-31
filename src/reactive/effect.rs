use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use super::{
    context::EffectContext,
    runtime::{Dependency, RuntimeInner},
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct EffectId {
    index: u32,
    generation: u32,
}

pub(super) struct EffectNode {
    effect: RefCell<Box<EffectCallback>>,
    state: RefCell<EffectState>,
    running: Cell<bool>,
}

type EffectCallback = dyn FnMut(&mut EffectContext<'_>);

struct EffectState {
    initialized: bool,
    dependencies: Vec<Dependency>,
}

impl EffectNode {
    pub(super) fn new(effect: impl FnMut(&mut EffectContext<'_>) + 'static) -> Self {
        Self {
            effect: RefCell::new(Box::new(effect)),
            state: RefCell::new(EffectState {
                initialized: false,
                dependencies: Vec::new(),
            }),
            running: Cell::new(false),
        }
    }

    pub(super) fn run_if_needed(&self, runtime: &RuntimeInner, id: EffectId) {
        let needs_run = {
            let state = self.state.borrow();
            !state.initialized
                || state
                    .dependencies
                    .iter()
                    .any(|dependency| dependency.changed(runtime))
        };
        if !needs_run {
            return;
        }

        let _guard = EffectRunGuard::enter(&self.running, id);
        let mut context = EffectContext::new(runtime);
        (self.effect.borrow_mut())(&mut context);
        self.state.replace(EffectState {
            initialized: true,
            dependencies: context.into_dependencies(),
        });
    }
}

struct EffectRunGuard<'a> {
    running: &'a Cell<bool>,
}

impl<'a> EffectRunGuard<'a> {
    fn enter(running: &'a Cell<bool>, id: EffectId) -> Self {
        assert!(
            !running.replace(true),
            "reactive Effect {id:?} was entered recursively"
        );
        Self { running }
    }
}

impl Drop for EffectRunGuard<'_> {
    fn drop(&mut self) {
        self.running.set(false);
    }
}

pub(super) struct EffectArena {
    slots: Vec<EffectSlot>,
    free: Vec<u32>,
    next_order: u64,
    len: usize,
}

struct EffectSlot {
    generation: u32,
    order: u64,
    effect: Option<Rc<EffectNode>>,
}

impl EffectArena {
    pub(super) fn new() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
            next_order: 0,
            len: 0,
        }
    }

    pub(super) fn insert(&mut self, effect: Rc<EffectNode>) -> EffectId {
        let order = self.next_order;
        self.next_order = self
            .next_order
            .checked_add(1)
            .expect("reactive Effect registration order exhausted");

        let id = if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[index as usize];
            debug_assert!(slot.effect.is_none());
            slot.order = order;
            slot.effect = Some(effect);
            EffectId {
                index,
                generation: slot.generation,
            }
        } else {
            let index = u32::try_from(self.slots.len()).expect("reactive Effect capacity exceeded");
            self.slots.push(EffectSlot {
                generation: 0,
                order,
                effect: Some(effect),
            });
            EffectId {
                index,
                generation: 0,
            }
        };
        self.len += 1;
        id
    }

    pub(super) fn remove(&mut self, id: EffectId) -> Option<Rc<EffectNode>> {
        let slot = self.slots.get_mut(id.index as usize)?;
        if slot.generation != id.generation {
            return None;
        }
        let effect = slot.effect.take()?;
        slot.generation = slot
            .generation
            .checked_add(1)
            .expect("reactive Effect generation exhausted");
        self.free.push(id.index);
        self.len -= 1;
        Some(effect)
    }

    pub(super) fn contains(&self, id: EffectId) -> bool {
        self.slots
            .get(id.index as usize)
            .is_some_and(|slot| slot.generation == id.generation && slot.effect.is_some())
    }

    pub(super) fn ordered(&self) -> Vec<(u64, EffectId, Rc<EffectNode>)> {
        let mut effects = self
            .slots
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| {
                let effect = slot.effect.as_ref()?;
                let index = u32::try_from(index).expect("reactive Effect index exceeded u32");
                Some((
                    slot.order,
                    EffectId {
                        index,
                        generation: slot.generation,
                    },
                    Rc::clone(effect),
                ))
            })
            .collect::<Vec<_>>();
        effects.sort_unstable_by_key(|(order, _, _)| *order);
        effects
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.len
    }
}
