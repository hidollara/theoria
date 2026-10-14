//! Resolve one entry point's dependency graph. No process-global macro state.
use crate::Error;
use organon_format::DefinitionFile;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

/// All definitions reachable from an entry point, with canonical source paths.
#[derive(Debug)]
pub struct LoadedDefinitions {
    pub file: DefinitionFile,
    /// Dependency order, including the entry point. Each file occurs once.
    pub paths: Vec<PathBuf>,
}

/// Resolve imports relative to their declaring file, deduplicating canonical paths.
pub fn load_definitions(entry: impl AsRef<Path>) -> Result<LoadedDefinitions, Error> {
    let mut loader = Loader::default();
    loader.visit(entry.as_ref())?;
    Ok(LoadedDefinitions {
        file: DefinitionFile {
            imports: vec![],
            definitions: loader.definitions,
        },
        paths: loader.paths,
    })
}

#[derive(Default)]
struct Loader {
    active: Vec<PathBuf>,
    visited: HashSet<PathBuf>,
    origins: HashMap<String, PathBuf>,
    definitions: Vec<organon_format::Definition>,
    paths: Vec<PathBuf>,
}

impl Loader {
    fn visit(&mut self, requested: &Path) -> Result<(), Error> {
        let path = fs::canonicalize(requested)
            .map_err(|e| Error(format!("{}: {e}", requested.display())))?;
        if let Some(start) = self.active.iter().position(|p| p == &path) {
            let chain = self.active[start..]
                .iter()
                .chain(std::iter::once(&path))
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(" -> ");
            return Err(Error(format!("cyclic imports: {chain}")));
        }
        if self.visited.contains(&path) {
            return Ok(());
        }
        if self.active.len() >= 128 {
            return Err(Error(format!(
                "{}: import nesting exceeds 128 files",
                path.display()
            )));
        }
        let source =
            fs::read_to_string(&path).map_err(|e| Error(format!("{}: {e}", path.display())))?;
        let file = DefinitionFile::from_xml(&source)
            .map_err(|e| Error(format!("{}: {e}", path.display())))?;
        self.active.push(path.clone());
        for import in &file.imports {
            self.visit(
                &path
                    .parent()
                    .expect("canonical file has a parent")
                    .join(import),
            )
            .map_err(|e| Error(format!("{}: import {import:?}: {e}", path.display())))?;
        }
        for definition in file.definitions {
            if let Some(previous) = self
                .origins
                .insert(definition.name().to_owned(), path.clone())
            {
                return Err(Error(format!(
                    "duplicate definition {:?} in {} and {}",
                    definition.name(),
                    previous.display(),
                    path.display()
                )));
            }
            self.definitions.push(definition);
        }
        self.active.pop();
        self.visited.insert(path.clone());
        self.paths.push(path);
        Ok(())
    }
}
