//! Publish a definition from the current source scope under a public name.
use super::Identifier;

/// Publish a local or imported definition from an Organon module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Export {
    pub name: Identifier,
    pub alias: Option<Identifier>,
}

impl Export {
    pub fn public_name(&self) -> &str {
        self.alias.as_deref().unwrap_or(&self.name)
    }
}
