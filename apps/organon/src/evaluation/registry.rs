//! App-owned adapters for constructing native elements and inspecting their outputs.
use organon_theoria::attribute;
use std::{any::Any, collections::BTreeMap, rc::Rc};

pub type Outputs = BTreeMap<String, organon_theoria::flow::Outcome<Value>>;

#[derive(Clone)]
pub struct Object {
    pub outputs: Outputs,
    pub interfaces: BTreeMap<String, Outputs>,
}

#[derive(Clone)]
pub struct Value {
    pub(super) native: attribute::Value,
    object: Option<Rc<Object>>,
}

impl Value {
    pub fn new<T: 'static>(value: T) -> Self {
        Self::from_native(attribute::Value::new(value))
    }

    pub fn from_native(native: attribute::Value) -> Self {
        Self {
            native,
            object: None,
        }
    }

    pub fn with_object(mut self, object: Object) -> Self {
        self.object = Some(Rc::new(object));
        self
    }

    pub fn get<T: Clone + 'static>(&self) -> Result<T, String> {
        self.native.get().or_else(|error| {
            self.object
                .as_ref()
                .and_then(|o| (o.as_ref() as &dyn Any).downcast_ref::<T>())
                .cloned()
                .ok_or(error)
        })
    }

    pub(super) fn resolved(
        self,
        ty: &TypeAdapter,
        source: &str,
        registry: &attribute::Registry,
    ) -> Result<Self, String> {
        let native = (ty.resolve)(&self.native.clone().with_contract(registry, source))?;
        let object = if native.type_id() == self.native.type_id() {
            self.object
        } else {
            None
        };
        Ok(Self { native, object })
    }
}

pub type EvaluateEntry = fn(
    Vec<Option<organon_theoria::flow::Outcome<Value>>>,
    Vec<Value>,
    &attribute::Registry,
) -> Result<Value, String>;
pub type PredicateEntry = fn(
    Vec<organon_theoria::flow::Outcome<Value>>,
    &attribute::Registry,
) -> organon_theoria::flow::Outcome<bool>;
pub type InterfaceEntry = fn(&Outputs, &attribute::Registry) -> Result<attribute::Value, String>;

#[derive(Clone)]
pub struct Entry {
    pub inputs: Vec<&'static str>,
    pub evaluate: EvaluateEntry,
}

/// Type-erased parser and resolver calls are needed only by the IR interpreter.
#[derive(Clone)]
pub struct TypeAdapter {
    pub name: &'static str,
    pub rust_type: fn() -> std::any::TypeId,
    pub interpret:
        fn(&str, &attribute::Scope) -> Result<attribute::Attribute<attribute::Value>, String>,
    pub resolve: fn(&attribute::Value) -> Result<attribute::Value, String>,
}
impl TypeAdapter {
    pub fn of<M: organon_theoria::Type>(name: &'static str) -> Self {
        Self {
            name,
            rust_type: std::any::TypeId::of::<M::Value>,
            interpret: |text, scope| {
                M::interpret(text, scope).map(|a| attribute::Attribute {
                    dependencies: a.dependencies,
                    source: a.source.map(|v| attribute::Value::new(v.clone())),
                })
            },
            resolve: |value| M::resolve(value).map(attribute::Value::new),
        }
    }
}

#[derive(Clone, Default)]
pub struct Registry {
    pub attributes: attribute::Registry,
    pub types: BTreeMap<String, TypeAdapter>,
    pub elements: BTreeMap<String, Entry>,
    pub predicates: BTreeMap<String, PredicateEntry>,
    pub projections: BTreeMap<(String, String), fn(Value) -> organon_theoria::flow::Outcome<Value>>,
    pub interfaces: BTreeMap<String, InterfaceEntry>,
}

impl Registry {
    pub fn alias(&mut self, local: &str, original: &str) -> Result<(), String> {
        if let Some(p) = self.predicates.get(original).copied() {
            self.predicates.insert(local.into(), p);
            return Ok(());
        }
        let attribute = self.attributes.alias(local, original);
        if let Some(ty) = self.types.get(original).cloned() {
            self.types.insert(local.into(), ty);
        }
        if let Some(entry) = self.elements.get(original).cloned() {
            self.elements.insert(local.into(), entry);
            Ok(())
        } else {
            attribute
        }
    }

    pub(super) fn value(&self, object: Object) -> Result<Value, String> {
        // The app converts interpreted outputs into ordinary, typed interface values.
        // Attribute interpretation only sees the interface provider, never this dictionary.
        #[derive(Clone)]
        struct Interfaces(BTreeMap<String, Result<attribute::Value, String>>);
        let mut provided = BTreeMap::new();
        for (name, outputs) in &object.interfaces {
            let convert = self
                .interfaces
                .get(name)
                .ok_or_else(|| format!("missing interface adapter {name:?}"))?;
            provided.insert(name.clone(), convert(outputs, &self.attributes));
        }
        let native =
            attribute::Value::new(Interfaces(provided)).with_provider(|value, interface| {
                value
                    .get::<Interfaces>()?
                    .0
                    .get(interface)
                    .cloned()
                    .ok_or_else(|| format!("missing interface {interface:?}"))?
            });
        Ok(Value::from_native(native).with_object(object))
    }
}
