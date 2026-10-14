//! Named Elements and output destructuring shared by evaluation scopes.
use super::{ElementExpression, Identifier};
use crate::Error;
use std::collections::BTreeMap;

/// One named Element in the enclosing evaluation scope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Let {
    pub name: Identifier,
    pub element: ElementExpression,
}

/// Destructure one Element into names in the enclosing evaluation scope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bind {
    /// (public output name, local binding name).
    pub outputs: BTreeMap<Identifier, Identifier>,
    pub element: ElementExpression,
}

/// A let or bind declaration in the surrounding evaluation scope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Binding {
    Let(Let),
    Bind(Bind),
}

impl Let {
    pub(super) fn validate(&self, depth: usize) -> Result<(), Error> {
        self.element.validate(depth)
    }
}

impl Bind {
    pub(super) fn validate(&self, depth: usize) -> Result<(), Error> {
        self.element.validate(depth)?;
        if self.outputs.is_empty() {
            return Err(Error("bind requires at least one output binding".into()));
        }
        Ok(())
    }
}

impl Binding {
    pub(crate) fn validate(&self, depth: usize) -> Result<(), Error> {
        match self {
            Self::Let(value) => value.validate(depth),
            Self::Bind(value) => value.validate(depth),
        }
    }
}
