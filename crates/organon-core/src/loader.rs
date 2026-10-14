//! Source-scoped import and publication resolution. Package discovery is supplied
//! by the host; this loader does not know Cargo, native implementations, or graphics.
use crate::Error;
use crate::{syntax, types::TypeName};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Debug,
};

/// The host owns source identity, relative imports and dependency lookup.
/// An ID must remain stable throughout one load. Equal IDs denote the same module.
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
#[derive(Clone, Debug)]
pub struct Loaded<Id> {
    /// Flattened, uniquely named definitions for semantic analysis.
    pub definitions: syntax::Module,
    pub public: BTreeMap<String, String>,
    pub origins: BTreeMap<String, DefinitionId<Id>>,
    pub source_order: Vec<Id>,
    /// Unresolved content read once per source, before import names are rewritten.
    pub sources: BTreeMap<Id, syntax::Module>,
}
struct Scope<Id> {
    names: BTreeMap<String, DefinitionId<Id>>,
    public: BTreeMap<String, DefinitionId<Id>>,
}
struct Loader<'a, S: Sources> {
    sources: &'a mut S,
    scopes: BTreeMap<S::Id, Scope<S::Id>>,
    definitions: BTreeMap<DefinitionId<S::Id>, syntax::Definition>,
    order: Vec<DefinitionId<S::Id>>,
    active: Vec<S::Id>,
    source_order: Vec<S::Id>,
    snapshots: BTreeMap<S::Id, syntax::Module>,
}
pub fn load<S: Sources>(entry: S::Id, sources: &mut S) -> Result<Loaded<S::Id>, Error> {
    let mut loader = Loader {
        sources,
        scopes: BTreeMap::new(),
        definitions: BTreeMap::new(),
        order: vec![],
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
            self.order.push(id.clone());
            self.definitions.insert(id, definition.clone());
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
    fn finish(mut self, entry: &S::Id) -> Result<Loaded<S::Id>, Error> {
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
        for id in self.definitions.keys() {
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
        let mut definitions = vec![];
        let mut origins = BTreeMap::new();
        for source in &self.source_order {
            let scope = &self.scopes[source];
            for id in self.order.iter().filter(|id| &id.source == source) {
                let mut definition = self.definitions.remove(id).expect("loaded definition");
                let name = generated[id].clone();
                rewrite(&mut definition, &name, &scope.names, &generated)
                    .map_err(|e| Error(format!("{:?}: {}: {e}", source, id.name)))?;
                origins.insert(name, id.clone());
                definitions.push(syntax::Item::Definition(definition));
            }
        }
        let public = self.scopes[entry]
            .public
            .iter()
            .map(|(n, id)| (n.clone(), generated[id].clone()))
            .collect();
        Ok(Loaded {
            definitions: syntax::Module { items: definitions },
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
fn mapped<Id: Ord>(
    name: &str,
    scope: &BTreeMap<String, DefinitionId<Id>>,
    generated: &BTreeMap<DefinitionId<Id>, String>,
) -> Result<String, Error> {
    scope
        .get(name)
        .and_then(|id| generated.get(id))
        .cloned()
        .ok_or_else(|| Error(format!("unknown definition {name:?}; import it before use")))
}
fn rewrite<Id: Ord>(
    definition: &mut syntax::Definition,
    name: &str,
    scope: &BTreeMap<String, DefinitionId<Id>>,
    generated: &BTreeMap<DefinitionId<Id>, String>,
) -> Result<(), Error> {
    let ty = |kind: &mut TypeName| -> Result<(), Error> {
        *kind = TypeName::parse(&mapped(kind.spelling(), scope, generated)?)?;
        Ok(())
    };
    let port = |p: &mut crate::types::Port| -> Result<(), Error> {
        ty(&mut p.ty.kind)?;
        for error in &mut p.ty.errors {
            ty(error)?;
        }
        Ok(())
    };
    match definition {
        syntax::Definition::Element(e) => {
            e.name = name.parse()?;
            if let Some(children) = &mut e.children {
                ty(&mut children.kind)?;
            }
            for d in &mut e.members {
                match d {
                    syntax::ElementMember::Input(i) => port(i)?,
                    syntax::ElementMember::Let(l) => expression(&mut l.value, scope, generated)?,
                    syntax::ElementMember::Bind(b) => expression(&mut b.value, scope, generated)?,
                    syntax::ElementMember::Impl(i) => {
                        i.interface = mapped(&i.interface, scope, generated)?.parse()?
                    }
                    _ => (),
                }
            }
        }
        syntax::Definition::DeclareElement(e) => {
            e.name = name.parse()?;
            if let Some(children) = &mut e.children {
                ty(&mut children.kind)?;
            }
            for d in &mut e.members {
                match d {
                    syntax::DeclareElementMember::Input(p)
                    | syntax::DeclareElementMember::OptionalInput(p) => port(p)?,
                    syntax::DeclareElementMember::Output(o) => port(o)?,
                    syntax::DeclareElementMember::Implements(n) => {
                        *n = mapped(n, scope, generated)?.parse()?
                    }
                }
            }
        }
        syntax::Definition::Predicate(p) => {
            p.name = name.parse()?;
            for member in &mut p.members {
                match member {
                    syntax::PredicateMember::Input(i) => port(i)?,
                    syntax::PredicateMember::Binding(l) => local(l, scope, generated)?,
                    syntax::PredicateMember::Predicate(c) => condition(c, scope, generated)?,
                }
            }
        }
        syntax::Definition::DeclarePredicate(p) => {
            p.name = name.parse()?;
            for member in &mut p.members {
                match member {
                    syntax::DeclarePredicateMember::Input(input) => port(input)?,
                    syntax::DeclarePredicateMember::Error(reason) => ty(reason)?,
                }
            }
        }
        syntax::Definition::Interface(i) => {
            i.name = name.parse()?;
            for p in &mut i.outputs {
                port(p)?;
            }
        }
    }
    Ok(())
}
fn expression<Id: Ord>(
    value: &mut syntax::ValueExpression,
    scope: &BTreeMap<String, DefinitionId<Id>>,
    generated: &BTreeMap<DefinitionId<Id>, String>,
) -> Result<(), Error> {
    match value {
        syntax::ValueExpression::Construct(i) => {
            i.element = mapped(&i.element, scope, generated)?.parse()?;
            for child in &mut i.children {
                expression(child, scope, generated)?;
            }
        }
        syntax::ValueExpression::Select(s) => {
            rename_type(&mut s.result_type, scope, generated)?;
            for b in &mut s.cases {
                branch(b, scope, generated)?;
            }
        }
        syntax::ValueExpression::Try(t) => {
            rename_type(&mut t.result_type, scope, generated)?;
            for item in &mut t.members {
                match item {
                    syntax::TryMember::Binding(l) => local(l, scope, generated)?,
                    syntax::TryMember::Unreachable { reason } => {
                        if let Some(reason) = reason {
                            rename_type(reason, scope, generated)?;
                        }
                    }
                    syntax::TryMember::Catch(c) => {
                        if let Some(t) = &mut c.reason {
                            rename_type(t, scope, generated)?;
                        }
                        branch(&mut c.branch, scope, generated)?;
                    }
                }
            }
        }
        syntax::ValueExpression::Reference { ty: Some(ty), .. } => {
            rename_type(ty, scope, generated)?
        }
        syntax::ValueExpression::Reference { ty: None, .. } | syntax::ValueExpression::Children => {
        }
    }
    Ok(())
}
fn rename_type<Id: Ord>(
    ty: &mut TypeName,
    scope: &BTreeMap<String, DefinitionId<Id>>,
    generated: &BTreeMap<DefinitionId<Id>, String>,
) -> Result<(), Error> {
    *ty = TypeName::parse(&mapped(ty.spelling(), scope, generated)?)?;
    Ok(())
}
fn condition<Id: Ord>(
    c: &mut syntax::PredicateExpression,
    scope: &BTreeMap<String, DefinitionId<Id>>,
    generated: &BTreeMap<DefinitionId<Id>, String>,
) -> Result<(), Error> {
    match c {
        syntax::PredicateExpression::Call { name, .. } => {
            *name = mapped(name, scope, generated)?.parse()?
        }
        syntax::PredicateExpression::Not(p) => condition(p, scope, generated)?,
        syntax::PredicateExpression::All(ps) | syntax::PredicateExpression::Any(ps) => {
            for p in ps {
                condition(p, scope, generated)?;
            }
        }
    }
    Ok(())
}
fn local<Id: Ord>(
    l: &mut syntax::LocalBinding,
    scope: &BTreeMap<String, DefinitionId<Id>>,
    generated: &BTreeMap<DefinitionId<Id>, String>,
) -> Result<(), Error> {
    expression(
        match l {
            syntax::LocalBinding::Let(v) => &mut v.value,
            syntax::LocalBinding::Bind(v) => &mut v.value,
        },
        scope,
        generated,
    )
}
fn branch<Id: Ord>(
    b: &mut syntax::Branch,
    scope: &BTreeMap<String, DefinitionId<Id>>,
    generated: &BTreeMap<DefinitionId<Id>, String>,
) -> Result<(), Error> {
    if let Some(t) = &mut b.exit.throw {
        rename_type(t, scope, generated)?;
    }
    for m in &mut b.members {
        match m {
            syntax::BranchMember::Binding(l) => local(l, scope, generated)?,
            syntax::BranchMember::Predicate(c) => condition(c, scope, generated)?,
        }
    }
    Ok(())
}
