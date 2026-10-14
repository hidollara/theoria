//! Package-owned definitions, identities and paths between library providers.
use crate::CargoSources;
use organon_core::{Error, format as f, loader};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

pub fn identity(file: &Path, name: &str) -> Result<String, Error> {
    let file = std::fs::canonicalize(file).map_err(|e| Error(e.to_string()))?;
    let manifest = manifest(&file)?;
    let root = manifest.parent().unwrap();
    let identity = std::fs::read_to_string(&manifest)
        .map_err(|e| Error(e.to_string()))?
        .replace("\r\n", "\n");
    let key = format!(
        "{identity}:{}:{name}",
        file.strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/")
    );
    // FNV-1a (64-bit).
    let hash = key.bytes().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
    });
    Ok(format!("D{hash:016x}"))
}

/// Rust path of a declaration in a provider's generated contract module.
pub fn declaration_path(id: &loader::DefinitionId, provider: &str) -> Result<String, Error> {
    Ok(format!(
        "{provider}::__organon::{}",
        identity(&id.file, &id.name)?
    ))
}

pub fn manifest(file: &Path) -> Result<PathBuf, Error> {
    let file = std::fs::canonicalize(file).map_err(|e| Error(e.to_string()))?;
    fs::canonicalize(enclosing_manifest(&file)?).map_err(|e| Error(e.to_string()))
}

/// Authored imports and the checked construction graph share one loading snapshot.
pub struct LoadedLibrary {
    pub loaded: organon_core::loader::Loaded,
    pub program: organon_core::semantic::Program,
}

pub fn load(entry: impl AsRef<Path>) -> Result<LoadedLibrary, Error> {
    let loaded = organon_core::loader::load(entry, &mut CargoSources::default())?;
    let program = organon_core::semantic::resolve(&loaded.file)?;
    Ok(LoadedLibrary { loaded, program })
}

pub(crate) fn enclosing_manifest(file: &Path) -> Result<PathBuf, Error> {
    file.parent()
        .into_iter()
        .flat_map(Path::ancestors)
        .map(|p| p.join("Cargo.toml"))
        .find(|p| p.is_file())
        .ok_or_else(|| {
            Error(format!(
                "{}: crate import requires an enclosing Cargo.toml",
                file.display()
            ))
        })
}

impl CargoSources {
    /// Re-export dependency crates so transitive consumers can connect to them.
    pub fn dependency_exports(
        &mut self,
        owner: &Path,
        loaded: &loader::Loaded,
    ) -> Result<proc_macro2::TokenStream, Error> {
        let mut exports = Vec::new();
        for (alias, path) in self.library_dependencies(owner, loaded)? {
            if path.join("organon.xml").is_file() {
                let alias = quote::format_ident!("r#{}", alias.replace('-', "_"));
                exports.push(quote::quote!(pub use ::#alias as #alias;));
            }
        }
        Ok(quote::quote!(pub mod __dependencies { #(#exports)* }))
    }

    /// Provider paths when importing into a Cargo target. A binary or integration
    /// test accesses its package's library through the library crate name.
    pub fn import_provider_paths(
        &mut self,
        consumer_manifest: &Path,
        loaded: &loader::Loaded,
    ) -> Result<BTreeMap<PathBuf, String>, Error> {
        let mut paths = self.provider_paths(consumer_manifest, loaded)?;
        if let Some(name) = self.library_name(consumer_manifest)?
            && (std::env::var("CARGO_CRATE_NAME").ok().as_deref() != Some(&name)
                || std::env::var_os("CARGO_BIN_NAME").is_some())
        {
            for path in paths.values_mut() {
                if path == "crate" {
                    *path = format!("::{name}");
                }
            }
        }
        Ok(paths)
    }

    /// Resolve each declaration file to its provider through the Organon library connector.
    pub fn provider_paths(
        &mut self,
        consumer_manifest: &Path,
        loaded: &loader::Loaded,
    ) -> Result<BTreeMap<PathBuf, String>, Error> {
        let root = std::fs::canonicalize(consumer_manifest).map_err(|e| Error(e.to_string()))?;
        let mut known = BTreeMap::from([(root.clone(), "crate".to_owned())]);
        let mut pending = vec![root];
        while let Some(owner) = pending.pop() {
            let prefix = known[&owner].clone();
            for (alias, path) in self.library_dependencies(&owner, loaded)? {
                let entry = path.join("organon.xml");
                if !entry.is_file() {
                    continue;
                }
                let next = manifest(&entry)?;
                if known.contains_key(&next) {
                    continue;
                }
                let path = if prefix == "crate" {
                    format!("::{}", alias.replace('-', "_"))
                } else {
                    format!(
                        "{prefix}::__organon::__dependencies::{}",
                        alias.replace('-', "_")
                    )
                };
                known.insert(next.clone(), path);
                pending.push(next);
            }
        }
        loaded
            .file
            .definitions()
            .filter(|d| {
                !matches!(d, f::Definition::Element(_))
                    || manifest(&loaded.origins[d.name()].file)
                        .is_ok_and(|owner| known.contains_key(&owner))
            })
            .map(|d| &loaded.origins[d.name()])
            .map(|id| {
                let owner = manifest(&id.file)?;
                let path = known.get(&owner).ok_or_else(|| {
                    Error(format!(
                        "{} is not reachable through an Organon Cargo dependency",
                        owner.display()
                    ))
                })?;
                Ok((id.file.clone(), path.clone()))
            })
            .collect()
    }

    /// Discover local dependencies and crate imports used by the definition graph.
    /// Unused registry dependencies do not need to be resolved for the host.
    fn library_dependencies(
        &mut self,
        owner: &Path,
        loaded: &loader::Loaded,
    ) -> Result<BTreeMap<String, PathBuf>, Error> {
        use organon_core::loader::Sources;
        let mut dependencies: BTreeMap<_, _> =
            self.local_dependencies(owner)?.into_iter().collect();
        for (file, source) in &loaded.snapshots {
            if manifest(file).ok().as_deref() != Some(owner) {
                continue;
            }
            let definition = f::DefinitionFile::from_xml(source)?;
            for source in definition.sources() {
                let f::ImportSource::Crate(name) = source else {
                    continue;
                };
                let entry = self.dependency(file, name)?;
                if manifest(&entry)? != owner {
                    dependencies.insert(name.clone(), entry.parent().unwrap().to_path_buf());
                }
            }
        }
        Ok(dependencies)
    }
}
