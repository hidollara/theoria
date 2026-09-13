use std::{cell::RefCell, rc::Rc};

use theoria_event::Dispatcher;
use theoria_reactive::{Effect, Reactive, ReactiveError, WriteContext};

use crate::{
    effect::EffectScheduler,
    node::{ConstructingComponent, MountedComponent},
};

/// Owns the root and shared state of one mounted Component tree.
pub(crate) struct ComponentTree {
    root: RefCell<Option<MountedComponent>>,
    effects: RefCell<EffectScheduler>,
    reactive: Reactive,
    default_dispatcher: Dispatcher,
}

impl ComponentTree {
    pub(crate) fn new(
        reactive: Reactive,
        default_dispatcher: Dispatcher,
        build: impl FnOnce(&Rc<Self>, &mut ConstructingComponent),
    ) -> Rc<Self> {
        let tree = Rc::new(Self {
            root: RefCell::new(None),
            effects: RefCell::new(EffectScheduler::new()),
            reactive,
            default_dispatcher,
        });
        let mut root = ConstructingComponent::new();
        build(&tree, &mut root);
        tree.root
            .borrow_mut()
            .replace(MountedComponent::complete(root));
        tree
    }

    pub(crate) fn default_dispatcher(&self) -> &Dispatcher {
        &self.default_dispatcher
    }

    pub(crate) fn reactive(&self) -> &Reactive {
        &self.reactive
    }

    pub(crate) fn sync_effect(&self, owner: &ConstructingComponent, effect: &Effect) {
        let sync = self.effects.borrow_mut().sync(owner, effect);
        owner.retain(sync);
    }

    pub(crate) fn request_effect(&self, owner: &MountedComponent, effect: &Effect) {
        self.effects.borrow_mut().request(owner, effect);
    }

    pub(crate) fn flush_effects(&self) -> Result<(), ReactiveError> {
        self.effects.borrow_mut().schedule_stale();
        loop {
            let effect = self.effects.borrow_mut().pop();
            let Some(effect) = effect else {
                break;
            };
            effect.run()?;
        }
        Ok(())
    }

    pub(crate) fn has_root(&self) -> bool {
        self.root.borrow().is_some()
    }

    pub(crate) fn request_unmount(&self, node: &MountedComponent) {
        if !node.unmount() {
            return;
        }
        let is_root = self
            .root
            .borrow()
            .as_ref()
            .is_some_and(|root| root.ptr_eq(node));
        if is_root {
            self.root.borrow_mut().take();
        }
        node.detach_from_parent();
    }

    /// Runs one live Component update as an isolated Reactive mutation.
    pub(crate) fn update<R>(
        &self,
        component: MountedComponent,
        update: impl FnOnce(WriteContext<'_>, MountedComponent) -> R,
    ) -> Option<R> {
        component
            .is_live()
            .then(|| self.mutate(|mutation| update(mutation, component)))
    }

    fn mutate<R>(&self, operation: impl FnOnce(WriteContext<'_>) -> R) -> R {
        self.reactive
            .mutate(operation)
            .expect("Component delivery does not overlap reactive evaluation")
    }
}

impl Drop for ComponentTree {
    fn drop(&mut self) {
        if let Some(root) = self.root.get_mut().take() {
            root.unmount();
        }
    }
}
