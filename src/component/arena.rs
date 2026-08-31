use std::any::Any;

use crate::reactive::ReactiveScope;

use super::{Component, ComponentContext, ComponentResource};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct ComponentId {
    index: u32,
    generation: u32,
}

pub(super) trait ErasedComponent {
    fn receive_erased(&mut self, event: Box<dyn Any>, context: &mut ComponentContext<'_>);
}

pub(super) struct TypedComponent<C: Component>(pub(super) C);

impl<C: Component> ErasedComponent for TypedComponent<C> {
    fn receive_erased(&mut self, event: Box<dyn Any>, context: &mut ComponentContext<'_>) {
        let event = event.downcast::<C::Event>().unwrap_or_else(|_| {
            panic!(
                "component event queue contained the wrong event type for {}",
                std::any::type_name::<C>()
            )
        });
        self.0.receive(*event, context);
    }
}

pub(super) struct ComponentArena {
    slots: Vec<ComponentSlot>,
    free: Vec<u32>,
    len: usize,
}

struct ComponentSlot {
    generation: u32,
    component: Option<Box<dyn ErasedComponent>>,
    scope: Option<ReactiveScope>,
    resources: Vec<Box<dyn ComponentResource>>,
    owner: Option<ComponentId>,
    children: Vec<ComponentId>,
    dispatching: bool,
    unmount_requested: bool,
}

impl ComponentArena {
    pub(super) fn new() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
            len: 0,
        }
    }

    pub(super) fn insert(
        &mut self,
        component: Box<dyn ErasedComponent>,
        scope: ReactiveScope,
        resources: Vec<Box<dyn ComponentResource>>,
    ) -> ComponentId {
        let id = if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[index as usize];
            debug_assert!(slot.component.is_none() && slot.scope.is_none());
            slot.component = Some(component);
            slot.scope = Some(scope);
            slot.resources = resources;
            slot.owner = None;
            slot.children.clear();
            slot.dispatching = false;
            slot.unmount_requested = false;
            ComponentId {
                index,
                generation: slot.generation,
            }
        } else {
            let index =
                u32::try_from(self.slots.len()).expect("component Runtime capacity exceeded");
            self.slots.push(ComponentSlot {
                generation: 0,
                component: Some(component),
                scope: Some(scope),
                resources,
                owner: None,
                children: Vec::new(),
                dispatching: false,
                unmount_requested: false,
            });
            ComponentId {
                index,
                generation: 0,
            }
        };
        self.len += 1;
        id
    }

    pub(super) fn can_own_children(&self, id: ComponentId) -> bool {
        self.slot(id).is_some_and(|slot| !slot.unmount_requested)
    }

    pub(super) fn adopt(&mut self, owner: ComponentId, child: ComponentId) -> bool {
        if owner == child
            || !self.can_own_children(owner)
            || self
                .slot(child)
                .is_none_or(|slot| slot.owner.is_some() || slot.unmount_requested)
        {
            return false;
        }

        self.slot_mut(child)
            .expect("validated child must remain mounted")
            .owner = Some(owner);
        self.slot_mut(owner)
            .expect("validated owner must remain mounted")
            .children
            .push(child);
        true
    }

    pub(super) fn adopt_many(&mut self, owner: ComponentId, children: &[ComponentId]) {
        for &child in children {
            assert!(
                self.adopt(owner, child),
                "newly mounted child could not be attached to its owner"
            );
        }
    }

    pub(super) fn accepts_events(&self, id: ComponentId) -> bool {
        self.slot(id).is_some_and(|slot| !slot.unmount_requested)
    }

    pub(super) fn take_for_dispatch(
        &mut self,
        id: ComponentId,
    ) -> Option<Box<dyn ErasedComponent>> {
        let slot = self.slot_mut(id)?;
        if slot.unmount_requested || slot.dispatching {
            return None;
        }
        let component = slot.component.take()?;
        slot.dispatching = true;
        Some(component)
    }

    pub(super) fn finish_dispatch(
        &mut self,
        id: ComponentId,
        component: Box<dyn ErasedComponent>,
    ) -> Vec<RemovedComponent> {
        let unmount_requested = {
            let slot = self
                .slot_mut(id)
                .expect("dispatched component slot disappeared before handler completion");
            assert!(slot.dispatching, "component was not marked as dispatching");
            slot.dispatching = false;
            if !slot.unmount_requested {
                slot.component = Some(component);
                return Vec::new();
            }
            slot.component = Some(component);
            true
        };
        debug_assert!(unmount_requested);
        self.remove_tree(id)
    }

    pub(super) fn request_unmount(&mut self, id: ComponentId, defer: bool) -> UnmountResult {
        let Some(slot) = self.slot(id) else {
            return UnmountResult::NotMounted;
        };
        if slot.unmount_requested {
            return UnmountResult::NotMounted;
        }
        let dispatching = slot.dispatching;
        self.mark_unmount_requested(id);
        if dispatching || defer {
            return UnmountResult::Deferred;
        }
        UnmountResult::Removed(self.remove_tree(id))
    }

    pub(super) fn remove_pending(&mut self) -> Vec<RemovedComponent> {
        let pending = self
            .slots
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.scope.is_some() && slot.unmount_requested && !slot.dispatching)
            .map(|(index, slot)| ComponentId {
                index: u32::try_from(index).expect("component Runtime capacity exceeded"),
                generation: slot.generation,
            })
            .filter(|id| {
                self.slot(*id)
                    .and_then(|slot| slot.owner)
                    .and_then(|owner| self.slot(owner))
                    .is_none_or(|owner| !owner.unmount_requested)
            })
            .collect::<Vec<_>>();

        pending
            .into_iter()
            .flat_map(|id| self.remove_tree(id))
            .collect()
    }

    pub(super) fn remove_roots(&mut self, ids: &[ComponentId]) -> Vec<RemovedComponent> {
        ids.iter()
            .copied()
            .flat_map(|id| self.remove_tree(id))
            .collect()
    }

    fn mark_unmount_requested(&mut self, id: ComponentId) {
        let children = {
            let Some(slot) = self.slot_mut(id) else {
                return;
            };
            slot.unmount_requested = true;
            for resource in &slot.resources {
                resource.deactivate();
            }
            slot.children.clone()
        };
        for child in children {
            self.mark_unmount_requested(child);
        }
    }

    fn remove_tree(&mut self, id: ComponentId) -> Vec<RemovedComponent> {
        let Some(slot) = self.slot(id) else {
            return Vec::new();
        };
        if slot.dispatching {
            return Vec::new();
        }
        let children = slot.children.clone();
        let mut removed = Vec::new();
        for child in children {
            removed.extend(self.remove_tree(child));
        }
        if let Some(component) = self.remove_one(id) {
            removed.push(component);
        }
        removed
    }

    fn remove_one(&mut self, id: ComponentId) -> Option<RemovedComponent> {
        let (component, scope, resources, owner) = {
            let slot = self.slot_mut(id)?;
            if slot.dispatching || !slot.children.is_empty() {
                return None;
            }
            (
                slot.component.take()?,
                slot.scope.take()?,
                std::mem::take(&mut slot.resources),
                slot.owner.take(),
            )
        };
        if let Some(owner) = owner
            && let Some(slot) = self.slot_mut(owner)
        {
            slot.children.retain(|child| *child != id);
        }
        let slot = &mut self.slots[id.index as usize];
        slot.generation = slot
            .generation
            .checked_add(1)
            .expect("component generation exhausted");
        slot.unmount_requested = false;
        slot.children.clear();
        self.free.push(id.index);
        self.len -= 1;
        Some(RemovedComponent {
            component,
            scope,
            resources,
        })
    }

    pub(super) fn add_resource(
        &mut self,
        id: ComponentId,
        resource: Box<dyn ComponentResource>,
    ) -> bool {
        let Some(slot) = self.slot_mut(id) else {
            return false;
        };
        if slot.unmount_requested {
            return false;
        }
        slot.resources.push(resource);
        true
    }

    fn slot(&self, id: ComponentId) -> Option<&ComponentSlot> {
        let slot = self.slots.get(id.index as usize)?;
        (slot.generation == id.generation && slot.scope.is_some()).then_some(slot)
    }

    fn slot_mut(&mut self, id: ComponentId) -> Option<&mut ComponentSlot> {
        let slot = self.slots.get_mut(id.index as usize)?;
        (slot.generation == id.generation && slot.scope.is_some()).then_some(slot)
    }
}

impl Drop for ComponentArena {
    fn drop(&mut self) {
        let roots = self
            .slots
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.scope.is_some() && slot.owner.is_none())
            .map(|(index, slot)| ComponentId {
                index: u32::try_from(index).expect("component Runtime capacity exceeded"),
                generation: slot.generation,
            })
            .collect::<Vec<_>>();
        for component in self.remove_roots(&roots) {
            component.dispose();
        }
        debug_assert_eq!(
            self.len, 0,
            "component ownership forest contained an orphan"
        );
    }
}

pub(super) struct RemovedComponent {
    component: Box<dyn ErasedComponent>,
    scope: ReactiveScope,
    resources: Vec<Box<dyn ComponentResource>>,
}

impl RemovedComponent {
    pub(super) fn dispose(self) {
        drop(self.component);
        drop(self.resources);
        drop(self.scope);
    }
}

pub(super) enum UnmountResult {
    NotMounted,
    Deferred,
    Removed(Vec<RemovedComponent>),
}

impl UnmountResult {
    pub(super) fn was_mounted(&self) -> bool {
        !matches!(self, Self::NotMounted)
    }

    pub(super) fn dispose(self) {
        if let Self::Removed(removed) = self {
            for component in removed {
                component.dispose();
            }
        }
    }
}
