//! File-scoped import and publication resolution. Package discovery is supplied
//! by the host; this loader does not know Cargo, native implementations, or graphics.
use crate::Error;
use crate::{format as f, types::Type};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

pub trait Sources {
    fn dependency(&mut self, declaring_file: &Path, name: &str) -> Result<PathBuf, Error>;
}
pub struct FilesOnly;
impl Sources for FilesOnly {
    fn dependency(&mut self, _: &Path, name: &str) -> Result<PathBuf, Error> {
        Err(Error(format!(
            "dependency {name:?} requires a package resolver"
        )))
    }
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DefinitionId {
    pub file: PathBuf,
    pub name: String,
}
#[derive(Clone, Debug)]
pub struct Loaded {
    /// Flattened, uniquely named definitions for semantic analysis. Persistence
    /// uses the original authored files, never this generated representation.
    pub file: f::DefinitionFile,
    pub public: BTreeMap<String, String>,
    pub origins: BTreeMap<String, DefinitionId>,
    pub paths: Vec<PathBuf>,
    pub snapshots: BTreeMap<PathBuf, String>,
}
struct Scope {
    names: BTreeMap<String, DefinitionId>,
    public: BTreeMap<String, DefinitionId>,
}
struct Loader<'a> {
    sources: &'a mut dyn Sources,
    scopes: BTreeMap<PathBuf, Scope>,
    definitions: BTreeMap<DefinitionId, f::Definition>,
    order: Vec<DefinitionId>,
    active: Vec<PathBuf>,
    paths: Vec<PathBuf>,
    snapshots: BTreeMap<PathBuf, String>,
}
pub fn load(entry: impl AsRef<Path>, sources: &mut dyn Sources) -> Result<Loaded, Error> {
    let mut loader = Loader {
        sources,
        scopes: BTreeMap::new(),
        definitions: BTreeMap::new(),
        order: vec![],
        active: vec![],
        paths: vec![],
        snapshots: BTreeMap::new(),
    };
    let entry = loader.visit(entry.as_ref())?;
    loader.finish(&entry)
}
impl Loader<'_> {
    fn visit(&mut self, requested: &Path) -> Result<PathBuf, Error> {
        let path = fs::canonicalize(requested)
            .map_err(|e| Error(format!("{}: {e}", requested.display())))?;
        if self.active.contains(&path) {
            return Err(Error(format!(
                "cyclic imports: {} -> {}",
                self.active
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(" -> "),
                path.display()
            )));
        }
        if self.scopes.contains_key(&path) {
            return Ok(path);
        }
        if self.active.len() >= 128 {
            return Err(Error("import nesting exceeds 128 files".into()));
        }
        let source =
            fs::read_to_string(&path).map_err(|e| Error(format!("{}: {e}", path.display())))?;
        let file = f::DefinitionFile::from_xml(&source)
            .map_err(|e| Error(format!("{}: {e}", path.display())))?;
        file.validate()?;
        self.snapshots.insert(path.clone(), source);
        self.active.push(path.clone());
        let mut names = BTreeMap::new();
        for definition in file.definitions() {
            let name = definition.name();
            let id = DefinitionId {
                file: path.clone(),
                name: name.into(),
            };
            if names.contains_key(name) {
                return Err(Error(format!(
                    "{}: duplicate definition {name:?}",
                    path.display()
                )));
            }
            names.insert(name.into(), id.clone());
            self.order.push(id.clone());
            self.definitions.insert(id, definition.clone());
        }
        for declaration in &file.declarations {
            let (from, selection) = match declaration {
                f::FileDeclaration::Import(import) => (&import.from, None),
                f::FileDeclaration::Use(selection) => (&selection.from, Some(selection)),
                _ => continue,
            };
            let requested = match from {
                f::ImportSource::Relative(relative) => path.parent().unwrap().join(relative),
                f::ImportSource::Crate(name) => self.sources.dependency(&path, name)?,
            };
            let target = self.visit(&requested).map_err(|e| {
                Error(format!(
                    "{}: import {:?}: {e}",
                    path.display(),
                    from.as_str()
                ))
            })?;
            let public = &self.scopes[&target].public;
            let Some(selection) = selection else {
                for (name, id) in public {
                    insert(&mut names, name, id, &path)?;
                }
                continue;
            };
            let name = &selection.name;
            let id = public.get(name).ok_or_else(|| {
                Error(format!(
                    "{}: {:?} does not publish {name:?}",
                    path.display(),
                    from.as_str()
                ))
            })?;
            insert(
                &mut names,
                selection.alias.as_deref().unwrap_or(name),
                id,
                &path,
            )?;
        }
        let mut public = BTreeMap::new();
        for declaration in &file.declarations {
            let f::FileDeclaration::Export(publication) = declaration else {
                continue;
            };
            let id = names.get(&publication.name).ok_or_else(|| {
                Error(format!(
                    "{}: unknown public definition {:?}",
                    path.display(),
                    publication.name
                ))
            })?;
            if public
                .insert(publication.public_name().into(), id.clone())
                .is_some()
            {
                return Err(Error(format!(
                    "{}: duplicate public name {:?}",
                    path.display(),
                    publication.public_name()
                )));
            }
        }
        self.scopes.insert(path.clone(), Scope { names, public });
        self.paths.push(path.clone());
        self.active.pop();
        Ok(path)
    }
    fn finish(self, entry: &Path) -> Result<Loaded, Error> {
        let mut generated: BTreeMap<DefinitionId, String> = BTreeMap::new();
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
        for path in &self.paths {
            let scope = &self.scopes[path];
            for id in self.order.iter().filter(|id| &id.file == path) {
                let definition = &self.definitions[id];
                let mut definition = definition.clone();
                let name = generated[id].clone();
                rewrite(&mut definition, &name, &scope.names, &generated)
                    .map_err(|e| Error(format!("{}: {}: {e}", path.display(), id.name)))?;
                origins.insert(name, id.clone());
                definitions.push(f::FileDeclaration::Definition(definition));
            }
        }
        let public = self.scopes[entry]
            .public
            .iter()
            .map(|(n, id)| (n.clone(), generated[id].clone()))
            .collect();
        Ok(Loaded {
            file: f::DefinitionFile {
                declarations: definitions,
            },
            public,
            origins,
            paths: self.paths,
            snapshots: self.snapshots,
        })
    }
}
fn insert(
    names: &mut BTreeMap<String, DefinitionId>,
    name: &str,
    id: &DefinitionId,
    path: &Path,
) -> Result<(), Error> {
    if let Some(previous) = names.get(name)
        && previous != id
    {
        return Err(Error(format!(
            "{}: duplicate definition {name:?} from {} and {}",
            path.display(),
            previous.file.display(),
            id.file.display()
        )));
    }
    names.insert(name.into(), id.clone());
    Ok(())
}
fn mapped(
    name: &str,
    scope: &BTreeMap<String, DefinitionId>,
    generated: &BTreeMap<DefinitionId, String>,
) -> Result<String, Error> {
    scope
        .get(name)
        .and_then(|id| generated.get(id))
        .cloned()
        .ok_or_else(|| Error(format!("unknown definition {name:?}; import it before use")))
}
fn rewrite(
    definition: &mut f::Definition,
    name: &str,
    scope: &BTreeMap<String, DefinitionId>,
    generated: &BTreeMap<DefinitionId, String>,
) -> Result<(), Error> {
    let ty = |kind: &mut Type| -> Result<(), Error> {
        *kind = Type::parse(&mapped(kind.spelling(), scope, generated)?)?;
        Ok(())
    };
    match definition {
        f::Definition::Element(e) => {
            e.name = name.into();
            for d in &mut e.declarations {
                match d {
                    f::Declaration::Input(i) => ty(&mut i.kind)?,
                    f::Declaration::Let(l) => expression(&mut l.value, scope, generated)?,
                    f::Declaration::Bind(b) => expression(&mut b.value, scope, generated)?,
                    f::Declaration::Impl(i) => {
                        i.interface = mapped(&i.interface, scope, generated)?
                    }
                    _ => (),
                }
            }
        }
        f::Definition::External(e) => {
            e.name = name.into();
            for d in &mut e.declarations {
                match d {
                    f::ExternalDeclaration::Input(p) | f::ExternalDeclaration::OptionalInput(p) => {
                        ty(&mut p.ty.kind)?
                    }
                    f::ExternalDeclaration::Children(c) => ty(&mut c.kind)?,
                    f::ExternalDeclaration::Impl(i) => {
                        i.interface = mapped(&i.interface, scope, generated)?
                    }
                    f::ExternalDeclaration::Output(o) => ty(&mut o.port.ty.kind)?,
                    f::ExternalDeclaration::Implements(n) => *n = mapped(n, scope, generated)?,
                }
            }
        }
        f::Definition::Interface(i) => {
            i.name = name.into();
            for p in &mut i.outputs {
                ty(&mut p.ty.kind)?;
            }
        }
    }
    Ok(())
}
fn expression(
    value: &mut f::ValueExpr,
    scope: &BTreeMap<String, DefinitionId>,
    generated: &BTreeMap<DefinitionId, String>,
) -> Result<(), Error> {
    if let f::ValueExpr::Instance(i) = value {
        i.element = mapped(&i.element, scope, generated)?;
        for child in &mut i.children {
            expression(child, scope, generated)?;
        }
    }
    Ok(())
}
