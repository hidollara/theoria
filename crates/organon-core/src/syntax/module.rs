//! Definitions, imports and publication in one source scope.
use super::{
    DeclareElement, DeclarePredicate, Element, Export, Import, ImportSource, Interface, Predicate,
    Use,
};
use crate::Error;

/// A source scope containing definitions and import/export directives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Module {
    /// Source order is retained; no XML layout, comments or attribute ordering is stored.
    pub items: Vec<Item>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    /// Import all public names from a relative or dependency source.
    Import(Import),
    /// Select one public name, optionally under a local alias.
    Use(Use),
    /// Publish a local or imported definition from this module.
    Export(Export),
    Definition(Definition),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Definition {
    Element(Element),
    DeclareElement(DeclareElement),
    Predicate(Predicate),
    DeclarePredicate(DeclarePredicate),
    Interface(Interface),
}

impl Definition {
    pub fn validate(&self) -> Result<(), Error> {
        match self {
            Self::Element(value) => value.validate(),
            Self::DeclareElement(value) => value.validate(),
            Self::Predicate(value) => value.validate(),
            Self::DeclarePredicate(value) => value.validate(),
            Self::Interface(value) => value.validate(),
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Element(value) => &value.name,
            Self::DeclareElement(value) => &value.name,
            Self::Predicate(value) => &value.name,
            Self::DeclarePredicate(value) => &value.name,
            Self::Interface(value) => &value.name,
        }
    }
}

impl Module {
    pub fn imports(&self) -> impl Iterator<Item = &Import> {
        self.items.iter().filter_map(|item| match item {
            Item::Import(v) => Some(v),
            _ => None,
        })
    }
    pub fn definitions(&self) -> impl Iterator<Item = &Definition> {
        self.items.iter().filter_map(|item| match item {
            Item::Definition(v) => Some(v),
            _ => None,
        })
    }
    pub fn uses(&self) -> impl Iterator<Item = &Use> {
        self.items.iter().filter_map(|item| match item {
            Item::Use(v) => Some(v),
            _ => None,
        })
    }
    pub fn sources(&self) -> impl Iterator<Item = &ImportSource> {
        self.items.iter().filter_map(|item| match item {
            Item::Import(v) => Some(&v.from),
            Item::Use(v) => Some(&v.from),
            _ => None,
        })
    }
    pub fn validate(&self) -> Result<(), Error> {
        for item in &self.items {
            match item {
                Item::Import(value) => value.validate()?,
                Item::Use(value) => value.validate()?,
                Item::Export(_) => (),
                Item::Definition(value) => value.validate()?,
            }
        }
        Ok(())
    }
}
