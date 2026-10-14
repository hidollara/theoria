//! Attribute interpretation is supplied by the receiving Rust type.

mod buffer;
mod parse;
mod template;
pub use buffer::ParseBuffer;
#[doc(hidden)]
pub use parse::contract;
pub use parse::{BindingType, Contract, ParseContext, ParseError, Reference, ReferenceParser};
pub use template::TextPart;
mod value;
use crate::{Calculation, Source};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};
pub use value::{Registry, TypeEntry, Value};
type Cell = Rc<RefCell<Option<Source<Value>>>>;

#[derive(Default)]
pub struct Scope {
    values: BTreeMap<String, Cell>,
    dependencies: BTreeMap<String, BTreeSet<String>>,
    registry: Rc<Registry>,
    contracts: BTreeMap<String, OwnedContract>,
}

struct OwnedContract {
    name: String,
    interfaces: Vec<String>,
}

impl Scope {
    /// Snapshot declarations without reading any Source. Used by runtime parsers.
    pub fn schema(&self) -> Vec<BindingType<'_>> {
        let mut bindings = Vec::new();
        for (name, contract) in &self.contracts {
            // Each entry records a contract this binding satisfies. Runtime-defined
            // interfaces need not have static names or native registry entries.
            for identity in std::iter::once(&contract.name).chain(&contract.interfaces) {
                bindings.push(BindingType {
                    name,
                    contract: Contract {
                        name: identity,
                        interfaces: &[],
                    },
                });
            }
        }
        bindings
    }

    pub fn new(registry: Registry) -> Self {
        Self {
            registry: Rc::new(registry),
            ..Self::default()
        }
    }

    /// Declare the Organon identity before interpreting references or setting a value.
    pub fn declare(&mut self, name: &str, contract: Contract<'_>) {
        self.values.entry(name.into()).or_default();
        self.contracts.insert(
            name.into(),
            OwnedContract {
                name: contract.name.into(),
                interfaces: contract.interfaces.iter().map(|s| (*s).into()).collect(),
            },
        );
    }

    pub fn declare_contract(&mut self, name: &str, contract: &str) -> Result<(), String> {
        let contract = self
            .registry
            .types
            .get(contract)
            .ok_or_else(|| format!("unregistered contract {contract:?}"))?
            .contract;
        self.declare(name, contract);
        Ok(())
    }

    pub(crate) fn clear(&self, name: &str) {
        self.values[name].borrow_mut().take();
    }

    pub fn set(&self, name: &str, source: Source<Value>) {
        let registry = self.registry.clone();
        let declaration = self.contracts[name].name.clone();
        *self.values[name].borrow_mut() =
            Some(source.map(move |v| v.clone().with_contract(&registry, &declaration)));
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

    fn require_contract<M: crate::Type>(&self, name: &str) -> Result<(), String> {
        let actual = self
            .contracts
            .get(name)
            .ok_or_else(|| format!("reference {name:?} has no declared contract"))?;
        if actual.name == M::ID || actual.interfaces.iter().any(|id| id == M::ID) {
            Ok(())
        } else {
            Err(format!("reference {name:?} does not satisfy {}", M::ID))
        }
    }

    fn reference(&self, name: &str) -> Result<Source<Value>, String> {
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
        .into_deferred())
    }
}

pub struct Attribute<T> {
    pub source: Source<T>,
    pub dependencies: Vec<String>,
}

impl<T: Clone + 'static> Attribute<T> {
    #[doc(hidden)]
    pub fn check_requirements(
        &self,
        schema: &[BindingType<'_>],
        selected: &[bool],
    ) -> Result<(), String> {
        for name in &self.dependencies {
            if !schema
                .iter()
                .zip(selected)
                .any(|(v, used)| *used && v.name == name)
            {
                return Err(format!(
                    "attribute parser read {name:?} outside its parsed requirements"
                ));
            }
        }
        Ok(())
    }
    pub fn reference<M: crate::Type<Value = T>>(text: &str, scope: &Scope) -> Result<Self, String> {
        scope.require_contract::<M>(text)?;
        let source = scope.reference(text)?;
        Ok(Self {
            dependencies: vec![text.into()],
            source: Calculation::new(move |cx| {
                let value = source.read(cx);
                M::resolve(&value).expect("validated reference violated its declared contract")
            })
            .into_deferred(),
        })
    }
}

/// The optional whole-reference spelling used by the built-in scalar parsers.
pub const fn braced_reference(text: &str) -> Option<&str> {
    let bytes = text.as_bytes();
    if bytes.len() >= 2 && bytes[0] == b'{' && bytes[bytes.len() - 1] == b'}' {
        let (_, rest) = text.split_at(1);
        Some(rest.split_at(rest.len() - 1).0)
    } else {
        None
    }
}

/// Parse once; the returned builder connects Display sources without reparsing.
pub const fn template<'a, const N: usize>(
    text: &'a str,
    cx: &mut ParseContext<'_>,
    mut parts: ParseBuffer<'a, TextPart<'a>, N>,
) -> Result<
    impl FnOnce(&mut ResolveContext<'_>) -> Result<Source<String>, String> + use<'a, N>,
    ParseError,
> {
    if let Err(error) = template::parse(text, cx, &mut parts) {
        return Err(error);
    }
    Ok(move |cx: &mut ResolveContext<'_>| {
        enum ResolvedPart {
            Literal(String),
            Reference(Source<crate::Display>),
        }
        let mut sources: BTreeMap<&str, Source<crate::Display>> = BTreeMap::new();
        let mut resolved = Vec::new();
        for part in parts.iter() {
            match part {
                TextPart::Literal(text) => resolved.push(ResolvedPart::Literal((*text).into())),
                TextPart::Reference(name) => {
                    let source = match sources.entry(name.as_str()) {
                        std::collections::btree_map::Entry::Occupied(entry) => entry.get().clone(),
                        std::collections::btree_map::Entry::Vacant(entry) => {
                            entry.insert(cx.reference::<crate::Display>(name)?).clone()
                        }
                    };
                    resolved.push(ResolvedPart::Reference(source));
                }
            }
        }
        if sources.is_empty() {
            let text: String = resolved
                .into_iter()
                .filter_map(|p| match p {
                    ResolvedPart::Literal(s) => Some(s),
                    _ => None,
                })
                .collect();
            return Ok(Source::from(text));
        }
        Ok(Calculation::new(move |cx| {
            let mut text = String::new();
            for part in &resolved {
                match part {
                    ResolvedPart::Literal(s) => text.push_str(s),
                    ResolvedPart::Reference(source) => {
                        text.push_str(&source.read(cx).text().read(cx))
                    }
                }
            }
            text
        })
        .into_deferred())
    })
}

/// Collect references while interpreting an attribute, before evaluating its value.
/// The marker selects an Organon contract, whose Value is the Rust value read later.
pub struct ResolveContext<'scope> {
    scope: &'scope Scope,
    dependencies: BTreeSet<String>,
}

impl<'scope> ResolveContext<'scope> {
    pub fn new(scope: &'scope Scope) -> Self {
        Self {
            scope,
            dependencies: BTreeSet::new(),
        }
    }

    pub(crate) fn reference<M: crate::Type>(
        &mut self,
        name: &str,
    ) -> Result<Source<M::Value>, String> {
        let attribute = Attribute::reference::<M>(name, self.scope)?;
        self.dependencies.extend(attribute.dependencies);
        Ok(attribute.source)
    }

    pub fn finish<T>(self, source: Source<T>) -> Attribute<T> {
        Attribute {
            source,
            dependencies: self.dependencies.into_iter().collect(),
        }
    }
}
