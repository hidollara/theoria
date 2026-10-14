//! Source-scoped import and publication resolution. Package discovery is supplied
//! by the host; this stage does not know Cargo, native implementations, or graphics.
use super::{Resolved, lower};
use crate::Error;
use crate::syntax;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Debug,
};

/// The host owns source identity, relative imports and dependency lookup.
/// An ID must remain stable throughout one resolution. Equal IDs denote the same module.
pub trait Sources {
    type Id: Clone + Ord + Debug;
    fn resolve(
        &mut self,
        from: &Self::Id,
        import: &syntax::ImportSource,
    ) -> Result<Self::Id, Error>;
    fn read(&mut self, id: &Self::Id) -> Result<syntax::Module, Error>;
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DefinitionId<Id> {
    pub source: Id,
    pub name: String,
}
struct Scope<Id> {
    names: BTreeMap<String, DefinitionId<Id>>,
    public: BTreeMap<String, DefinitionId<Id>>,
}
struct Loader<'a, S: Sources> {
    sources: &'a mut S,
    scopes: BTreeMap<S::Id, Scope<S::Id>>,
    definitions: BTreeSet<DefinitionId<S::Id>>,
    active: Vec<S::Id>,
    source_order: Vec<S::Id>,
    snapshots: BTreeMap<S::Id, syntax::Module>,
}
pub(super) fn resolve<S: Sources>(entry: S::Id, sources: &mut S) -> Result<Resolved<S::Id>, Error> {
    let mut loader = Loader {
        sources,
        scopes: BTreeMap::new(),
        definitions: BTreeSet::new(),
        active: vec![],
        source_order: vec![],
        snapshots: BTreeMap::new(),
    };
    let entry = loader.visit(entry)?;
    loader.finish(&entry)
}
impl<S: Sources> Loader<'_, S> {
    fn visit(&mut self, source: S::Id) -> Result<S::Id, Error> {
        if self.active.contains(&source) {
            return Err(Error(format!(
                "cyclic imports: {} -> {:?}",
                self.active
                    .iter()
                    .map(|p| format!("{:?}", p))
                    .collect::<Vec<_>>()
                    .join(" -> "),
                source
            )));
        }
        if self.scopes.contains_key(&source) {
            return Ok(source);
        }
        if self.active.len() >= 128 {
            return Err(Error("import nesting exceeds 128 sources".into()));
        }
        let module = self
            .sources
            .read(&source)
            .map_err(|e| Error(format!("{:?}: {e}", source)))?;
        module.validate()?;
        self.active.push(source.clone());
        let mut names = BTreeMap::new();
        for definition in module.definitions() {
            let name = definition.name();
            let id = DefinitionId {
                source: source.clone(),
                name: name.into(),
            };
            if names.contains_key(name) {
                return Err(Error(format!(
                    "{:?}: duplicate definition {name:?}",
                    source
                )));
            }
            names.insert(name.into(), id.clone());
            self.definitions.insert(id);
        }
        for item in &module.items {
            let (from, selection) = match item {
                syntax::Item::Import(import) => (&import.from, None),
                syntax::Item::Use(selection) => (&selection.from, Some(selection)),
                _ => continue,
            };
            let requested = self.sources.resolve(&source, from)?;
            let target = self
                .visit(requested)
                .map_err(|e| Error(format!("{:?}: import {:?}: {e}", source, from.as_str())))?;
            let public = &self.scopes[&target].public;
            let Some(selection) = selection else {
                for (name, id) in public {
                    insert(&mut names, name, id, &source)?;
                }
                continue;
            };
            let name = &selection.name;
            let id = public.get(name.as_str()).ok_or_else(|| {
                Error(format!(
                    "{:?}: {:?} does not publish {name:?}",
                    source,
                    from.as_str()
                ))
            })?;
            insert(
                &mut names,
                selection.alias.as_deref().unwrap_or(name),
                id,
                &source,
            )?;
        }
        let mut public = BTreeMap::new();
        for item in &module.items {
            let syntax::Item::Export(publication) = item else {
                continue;
            };
            let id = names.get(publication.name.as_str()).ok_or_else(|| {
                Error(format!(
                    "{:?}: unknown public definition {:?}",
                    source, publication.name
                ))
            })?;
            if public
                .insert(publication.public_name().into(), id.clone())
                .is_some()
            {
                return Err(Error(format!(
                    "{:?}: duplicate public name {:?}",
                    source,
                    publication.public_name()
                )));
            }
        }
        self.scopes.insert(source.clone(), Scope { names, public });
        self.source_order.push(source.clone());
        self.snapshots.insert(source.clone(), module);
        self.active.pop();
        Ok(source)
    }
    fn finish(self, entry: &S::Id) -> Result<Resolved<S::Id>, Error> {
        let mut generated: BTreeMap<DefinitionId<S::Id>, String> = BTreeMap::new();
        let mut used = BTreeSet::new();
        // Public names at the entry keep their spelling in generated APIs where possible.
        for (name, id) in self.scopes[entry]
            .public
            .iter()
            .chain(self.scopes[entry].names.iter())
        {
            if !generated.contains_key(id) && used.insert(name.clone()) {
                generated.insert(id.clone(), name.clone());
            }
        }
        for id in &self.definitions {
            if generated.contains_key(id) {
                continue;
            }
            let mut name = id.name.clone();
            if used.contains(&name) {
                name = format!("OrganonPrivate{}_{}", generated.len(), id.name);
            }
            while !used.insert(name.clone()) {
                name.push('_');
            }
            generated.insert(id.clone(), name);
        }
        let names: BTreeMap<_, _> = self
            .scopes
            .iter()
            .map(|(source, scope)| {
                (
                    source.clone(),
                    scope
                        .names
                        .iter()
                        .map(|(name, id)| (name.clone(), generated[id].clone()))
                        .collect::<BTreeMap<_, _>>(),
                )
            })
            .collect();
        let origins: BTreeMap<_, _> = generated
            .iter()
            .map(|(id, key)| (key.clone(), id.clone()))
            .collect();
        let definitions = origins
            .iter()
            .map(|(key, id)| {
                let definition = self.snapshots[&id.source]
                    .definitions()
                    .find(|d| d.name() == id.name)
                    .expect("collected definition");
                (
                    key.clone(),
                    lower::Definition {
                        syntax: definition,
                        names: &names[&id.source],
                        source: &id.source,
                    },
                )
            })
            .collect();
        let program = lower::resolve(definitions)?;
        let public = self.scopes[entry]
            .public
            .iter()
            .map(|(n, id)| (n.clone(), generated[id].clone()))
            .collect();
        Ok(Resolved {
            program,
            public,
            origins,
            source_order: self.source_order,
            sources: self.snapshots,
        })
    }
}
fn insert<Id: Clone + Ord + Debug>(
    names: &mut BTreeMap<String, DefinitionId<Id>>,
    name: &str,
    id: &DefinitionId<Id>,
    source: &Id,
) -> Result<(), Error> {
    if let Some(previous) = names.get(name)
        && previous != id
    {
        return Err(Error(format!(
            "{:?}: duplicate definition {name:?} from {:?} and {:?}",
            source, previous.source, id.source
        )));
    }
    names.insert(name.into(), id.clone());
    Ok(())
}
