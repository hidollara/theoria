use std::{
    collections::VecDeque,
    rc::{Rc, Weak},
};

use theoria_reactive::Effect;

use crate::node::{ComponentWeak, ConstructingComponent, MountedComponent};

/// Component-side Effect scheduling state.
pub(crate) struct EffectScheduler {
    pending: VecDeque<ComponentEffect>,
    syncs: Vec<Weak<ComponentEffect>>,
}

impl EffectScheduler {
    pub(crate) fn new() -> Self {
        Self {
            pending: VecDeque::new(),
            syncs: Vec::new(),
        }
    }

    pub(crate) fn request(&mut self, owner: &MountedComponent, effect: &Effect) {
        self.pending.push_back(ComponentEffect {
            owner: owner.weak(),
            effect: effect.clone(),
        });
    }

    pub(crate) fn sync(
        &mut self,
        owner: &ConstructingComponent,
        effect: &Effect,
    ) -> Rc<ComponentEffect> {
        let sync = Rc::new(ComponentEffect {
            owner: owner.weak(),
            effect: effect.clone(),
        });
        self.syncs.push(Rc::downgrade(&sync));
        sync
    }

    pub(crate) fn schedule_stale(&mut self) {
        let Self { pending, syncs } = self;
        syncs.retain(|sync| sync.strong_count() != 0);
        for sync in syncs.iter().filter_map(Weak::upgrade) {
            if sync.is_live()
                && sync.effect.is_stale()
                && !pending.iter().any(|pending| pending.effect == sync.effect)
            {
                pending.push_back(sync.as_ref().clone());
            }
        }
    }

    pub(crate) fn pop(&mut self) -> Option<Effect> {
        while let Some(pending) = self.pending.pop_front() {
            if pending.owner.upgrade().is_some() {
                return Some(pending.effect);
            }
        }
        None
    }
}

#[derive(Clone)]
pub(crate) struct ComponentEffect {
    owner: ComponentWeak,
    effect: Effect,
}

impl ComponentEffect {
    fn is_live(&self) -> bool {
        self.owner.upgrade().is_some()
    }
}
