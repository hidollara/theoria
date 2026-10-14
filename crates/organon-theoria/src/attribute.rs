//! Attribute interpretation is supplied by the receiving Rust type.

mod template;
mod value;
use crate::{Calculation, Source};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};
pub use value::{Registry, TypeEntry, Value};
type Cell = Rc<RefCell<Option<Source<Result<Value, &'static str>>>>>;

#[derive(Default)]
pub struct Scope {
    values: BTreeMap<String, Cell>,
    dependencies: BTreeMap<String, BTreeSet<String>>,
    registry: Rc<Registry>,
    types: RefCell<BTreeMap<String, std::any::TypeId>>,
    contracts: BTreeMap<String, Vec<std::any::TypeId>>,
    fallible: BTreeSet<String>,
}

impl Scope {
    pub fn new(registry: Registry) -> Self {
        Self {
            registry: Rc::new(registry),
            ..Self::default()
        }
    }

    pub fn declare(&mut self, name: &str) {
        self.values.entry(name.into()).or_default();
    }

    pub fn declare_typed<T: 'static>(&mut self, name: &str) {
        self.declare(name);
        self.types
            .borrow_mut()
            .insert(name.into(), std::any::TypeId::of::<T>());
    }

    pub fn declare_contract(&mut self, name: &str, contract: &str) {
        self.declare(name);
        if let Some(entry) = self.registry.types.get(contract) {
            self.types
                .borrow_mut()
                .insert(name.into(), (entry.rust_type)());
        }
    }

    pub fn declare_interfaces(&mut self, name: &str, interfaces: &[String]) {
        self.contracts.insert(
            name.into(),
            interfaces
                .iter()
                .filter_map(|n| self.registry.types.get(n).map(|e| (e.rust_type)()))
                .collect(),
        );
    }

    pub fn declare_fallible(&mut self, name: &str) {
        self.declare(name);
        self.fallible.insert(name.into());
    }

    pub fn set(&self, name: &str, source: Source<Value>) {
        if let Some(value) = source.as_fixed() {
            self.types
                .borrow_mut()
                .entry(name.into())
                .or_insert(value.type_id());
        }
        self.set_fallible(name, source.map(|v| Ok(v.clone())));
    }

    /// Store a fallible computation's result without treating Err as resolution failure.
    pub fn set_fallible(&self, name: &str, source: Source<Result<Value, &'static str>>) {
        let registry = self.registry.clone();
        *self.values[name].borrow_mut() =
            Some(source.map(move |v| v.clone().map(|v| v.with_registry(&registry))));
    }

    pub fn dependencies(&mut self, name: &str, refs: impl IntoIterator<Item = String>) {
        self.dependencies
            .entry(name.into())
            .or_default()
            .extend(refs);
    }

    pub fn order(&self) -> Result<Vec<String>, String> {
        fn visit(
            name: &str,
            graph: &BTreeMap<String, BTreeSet<String>>,
            active: &mut BTreeSet<String>,
            done: &mut BTreeSet<String>,
            out: &mut Vec<String>,
        ) -> Result<(), String> {
            if done.contains(name) || !graph.contains_key(name) {
                return Ok(());
            }
            if !active.insert(name.into()) {
                return Err(format!("cyclic binding {name:?}"));
            }
            for dependency in &graph[name] {
                visit(dependency, graph, active, done, out)?;
            }
            active.remove(name);
            done.insert(name.into());
            out.push(name.into());
            Ok(())
        }
        let mut result = vec![];
        let mut active = BTreeSet::new();
        let mut done = BTreeSet::new();
        for name in self.dependencies.keys() {
            visit(
                name,
                &self.dependencies,
                &mut active,
                &mut done,
                &mut result,
            )?;
        }
        Ok(result)
    }

    fn require_type<T: 'static>(&self, name: &str) -> Result<(), String> {
        let expected = std::any::TypeId::of::<T>();
        if self
            .contracts
            .get(name)
            .is_some_and(|v| v.contains(&expected))
        {
            return Ok(());
        }
        let types = self.types.borrow();
        let actual = types
            .get(name)
            .ok_or_else(|| format!("reference {name:?} has no declared type"))?;
        if *actual == expected
            || self
                .registry
                .types
                .values()
                .any(|entry| (entry.rust_type)() == *actual && entry.interfaces.contains(&expected))
        {
            Ok(())
        } else {
            Err(format!(
                "reference {name:?} does not satisfy {}",
                std::any::type_name::<T>()
            ))
        }
    }

    fn require<T: 'static>(&self, name: &str) -> Result<(), String> {
        if self.fallible.contains(name) {
            return Err(format!(
                "reference {name:?} is fallible and requires explicit error handling"
            ));
        }
        self.require_type::<T>(name)
    }

    fn reference(&self, name: &str) -> Result<Source<Result<Value, &'static str>>, String> {
        let slot = self
            .values
            .get(name)
            .ok_or_else(|| format!("unknown reference {name:?}"))?
            .clone();
        Ok(Calculation::new(move |cx| {
            let source = slot
                .borrow()
                .clone()
                .expect("validated dependency must be initialized before evaluation");
            source.read(cx).into_cloned()
        })
        .into_inline())
    }
}

pub struct Attribute<T> {
    pub source: Source<T>,
    pub dependencies: Vec<String>,
}

impl<T: Clone + 'static> Attribute<T> {
    pub fn reference<M: crate::Type<Value = T>>(text: &str, scope: &Scope) -> Result<Self, String> {
        scope.require::<T>(text)?;
        let source = scope.reference(text)?;
        Ok(Self {
            dependencies: vec![text.into()],
            source: Calculation::new(move |cx| {
                let value = source.read(cx);
                M::resolve(
                    value
                        .as_ref()
                        .expect("infallible reference produced a fallible value"),
                )
                .expect("validated reference violated its declared contract")
            })
            .into_inline(),
        })
    }

    pub fn parsed<M: crate::Type<Value = T>>(
        text: &str,
        scope: &Scope,
        parse: fn(&str) -> Result<T, &'static str>,
    ) -> Result<Self, String> {
        let mut cx = ParseContext::new(scope);
        let source = reference_or_literal::<M>(text, &mut cx, parse)?;
        Ok(cx.finish(source))
    }
}

/// Collect references while interpreting an attribute, before evaluating its value.
/// The marker selects an Organon contract, whose Value is the Rust value read later.
pub struct ParseContext<'scope> {
    scope: &'scope Scope,
    dependencies: BTreeSet<String>,
}

impl<'scope> ParseContext<'scope> {
    pub fn new(scope: &'scope Scope) -> Self {
        Self {
            scope,
            dependencies: BTreeSet::new(),
        }
    }

    pub fn reference<M: crate::Type>(&mut self, name: &str) -> Result<Source<M::Value>, String> {
        let attribute = Attribute::reference::<M>(name, self.scope)?;
        self.dependencies.extend(attribute.dependencies);
        Ok(attribute.source)
    }

    /// Request a fallible value explicitly. Infallible values are lifted into Ok.
    pub fn reference_fallible<M: crate::Type>(
        &mut self,
        name: &str,
    ) -> Result<Source<Result<M::Value, &'static str>>, String> {
        self.scope.require_type::<M::Value>(name)?;
        let source = self.scope.reference(name)?;
        self.dependencies.insert(name.into());
        Ok(source.map(|value| {
            value
                .as_ref()
                .map(|value| {
                    M::resolve(value).expect("validated reference violated its declared contract")
                })
                .map_err(|error| *error)
        }))
    }

    pub fn finish<T>(self, source: Source<T>) -> Attribute<T> {
        Attribute {
            source,
            dependencies: self.dependencies.into_iter().collect(),
        }
    }
}

/// Parse a whole typed reference (`{name}`), or pass the literal unchanged to its parser.
pub fn reference_or_literal<M: crate::Type>(
    text: &str,
    cx: &mut ParseContext<'_>,
    parse: fn(&str) -> Result<M::Value, &'static str>,
) -> Result<Source<M::Value>, String> {
    if let Some(name) = text.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
        return cx.reference::<M>(name);
    }
    Ok(Source::from(parse(text).map_err(str::to_owned)?))
}

/// Format a string, requiring Display for every embedded reference.
pub fn template(text: &str, cx: &mut ParseContext<'_>) -> Result<Source<String>, String> {
    use template::TextPart;
    let parts = template::parse(text)?;
    let mut sources = BTreeMap::new();
    for part in &parts {
        if let TextPart::Reference(name) = part {
            sources.insert(name.clone(), cx.reference::<crate::Display>(name)?);
        }
    }
    if sources.is_empty() {
        let text: String = parts
            .into_iter()
            .filter_map(|p| match p {
                TextPart::Literal(s) => Some(s),
                _ => None,
            })
            .collect();
        return Ok(Source::from(text));
    }
    Ok(Calculation::new(move |cx| {
        let mut text = String::new();
        for part in &parts {
            match part {
                TextPart::Literal(s) => text.push_str(s),
                TextPart::Reference(name) => {
                    let value = sources[name].read(cx);
                    text.push_str(&value.text().read(cx));
                }
            }
        }
        text
    })
    .into_inline())
}
