//! Resolve source imports, names and construction contracts without rewriting syntax.
mod imports;
mod lower;

use crate::{Error, semantic::Program, syntax};
pub use imports::{DefinitionId, Sources};
use std::collections::BTreeMap;

/// One resolution snapshot: checked IR and the unmodified sources it came from.
#[derive(Clone, Debug)]
pub struct Resolved<Id> {
    pub program: Program,
    pub public: BTreeMap<String, String>,
    pub origins: BTreeMap<String, DefinitionId<Id>>,
    pub source_order: Vec<Id>,
    pub sources: BTreeMap<Id, syntax::Module>,
}

pub fn resolve<S: Sources>(entry: S::Id, sources: &mut S) -> Result<Resolved<S::Id>, Error> {
    imports::resolve(entry, sources)
}

/// Resolve a self-contained module supplied directly by a host.
pub fn resolve_module(module: &syntax::Module) -> Result<Program, Error> {
    struct Single<'a>(&'a syntax::Module);
    impl Sources for Single<'_> {
        type Id = ();
        fn resolve(&mut self, _: &(), _: &syntax::ImportSource) -> Result<(), Error> {
            Err(Error("imports require a source provider".into()))
        }
        fn read(&mut self, _: &()) -> Result<syntax::Module, Error> {
            Ok(self.0.clone())
        }
    }
    Ok(resolve((), &mut Single(module))?.program)
}
