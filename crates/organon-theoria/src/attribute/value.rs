//! Native values and type contracts used by attribute interpretation.
use std::{
    any::{Any, TypeId},
    collections::BTreeMap,
    rc::Rc,
};

#[derive(Clone)]
pub struct Value {
    data: Rc<dyn Any>,
    provide: Option<ProvideValue>,
}
impl Value {
    pub fn new<T: 'static>(value: T) -> Self {
        Self {
            data: Rc::new(value),
            provide: None,
        }
    }
    pub fn type_id(&self) -> TypeId {
        self.data.as_ref().type_id()
    }
    /// Attach an implementation of the value's declared interface contracts.
    pub fn with_provider(mut self, provide: ProvideValue) -> Self {
        self.provide = Some(provide);
        self
    }
    pub fn with_registry(mut self, registry: &Registry) -> Self {
        if self.provide.is_none() {
            self.provide = registry
                .types
                .values()
                .find(|t| (t.rust_type)() == self.type_id())
                .map(|t| t.provide);
        }
        self
    }
    pub fn provide(&self, interface: &str) -> Result<Self, String> {
        self.provide.ok_or("value does not provide interfaces")?(self, interface)
    }
    pub fn get<T: Clone + 'static>(&self) -> Result<T, String> {
        self.data
            .downcast_ref::<T>()
            .cloned()
            .ok_or_else(|| format!("expected {}", std::any::type_name::<T>()))
    }
}
pub type ProvideValue = fn(&Value, &str) -> Result<Value, String>;
#[derive(Clone)]
pub struct TypeEntry {
    pub name: &'static str,
    pub rust_type: fn() -> TypeId,
    pub provide: ProvideValue,
    pub interfaces: Vec<TypeId>,
}
#[derive(Clone, Default)]
pub struct Registry {
    pub types: BTreeMap<String, TypeEntry>,
}
impl Registry {
    pub fn extend(&mut self, other: Self) {
        self.types.extend(other.types);
    }
    pub fn alias(&mut self, local: &str, original: &str) -> Result<(), String> {
        let value = self
            .types
            .get(original)
            .ok_or_else(|| format!("unregistered type {original:?}"))?
            .clone();
        self.types.insert(local.into(), value);
        Ok(())
    }
}
