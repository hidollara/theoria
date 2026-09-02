use std::{
    any::Any,
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

/// A Component lifetime boundary whose construction has not yet completed.
pub(crate) struct ConstructingComponent {
    node: Rc<ComponentNode>,
}

impl ConstructingComponent {
    pub(crate) fn new() -> Self {
        Self {
            node: ComponentNode::new(),
        }
    }

    pub(crate) fn adopt(&self, child: Self) {
        self.node.adopt(&child.node);
    }

    pub(crate) fn retain<T: 'static>(&self, value: T) {
        self.node.retained.borrow_mut().push(Box::new(value));
    }

    pub(crate) fn weak(&self) -> ComponentWeak {
        ComponentWeak(Rc::downgrade(&self.node))
    }
}

/// A Component lifetime boundary attached to its owning Component tree.
#[derive(Clone)]
pub(crate) struct MountedComponent {
    node: Rc<ComponentNode>,
}

impl MountedComponent {
    pub(crate) fn complete(component: ConstructingComponent) -> Self {
        let mounted = Self {
            node: component.node,
        };
        mounted.node.mount();
        mounted
    }

    pub(crate) fn mount(&self, child: ConstructingComponent) -> bool {
        if !self.is_live() {
            return false;
        }
        let child = Self { node: child.node };
        self.node.adopt(&child.node);
        child.node.mount();
        true
    }

    pub(crate) fn weak(&self) -> ComponentWeak {
        ComponentWeak(Rc::downgrade(&self.node))
    }

    pub(crate) fn unmount(&self) -> bool {
        self.node.unmount()
    }

    pub(crate) fn detach_from_parent(&self) {
        let parent = self
            .node
            .parent
            .borrow_mut()
            .take()
            .and_then(|parent| parent.upgrade());
        if let Some(parent) = parent {
            parent
                .children
                .borrow_mut()
                .retain(|child| !Rc::ptr_eq(child, &self.node));
        }
    }

    pub(crate) fn is_live(&self) -> bool {
        self.node.active.get()
    }

    pub(crate) fn ptr_eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.node, &other.node)
    }
}

struct ComponentNode {
    parent: RefCell<Option<Weak<ComponentNode>>>,
    children: RefCell<Vec<Rc<ComponentNode>>>,
    active: Cell<bool>,
    retained: RefCell<Vec<Box<dyn Any>>>,
}

impl ComponentNode {
    fn new() -> Rc<Self> {
        Rc::new(Self {
            parent: RefCell::new(None),
            children: RefCell::new(Vec::new()),
            active: Cell::new(false),
            retained: RefCell::new(Vec::new()),
        })
    }

    fn adopt(self: &Rc<Self>, child: &Rc<Self>) {
        child.parent.borrow_mut().replace(Rc::downgrade(self));
        self.children.borrow_mut().push(Rc::clone(child));
    }

    fn mount(&self) {
        if self.active.replace(true) {
            return;
        }
        for child in self.children.borrow().iter() {
            child.mount();
        }
    }

    fn unmount(&self) -> bool {
        if !self.active.replace(false) {
            return false;
        }
        for child in self.children.borrow().iter() {
            child.unmount();
        }
        true
    }
}

/// A non-owning reference that resolves only after construction and before unmount.
#[derive(Clone)]
pub(crate) struct ComponentWeak(Weak<ComponentNode>);

impl ComponentWeak {
    pub(crate) fn upgrade(&self) -> Option<MountedComponent> {
        let node = self.0.upgrade()?;
        node.active.get().then_some(MountedComponent { node })
    }
}
