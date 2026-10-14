//! Named values and output destructuring shared by evaluation scopes.
use super::ValueExpression;
use crate::Error;
use crate::identifier::Identifier;
use std::collections::BTreeMap;

/// One named value in the enclosing evaluation scope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Let {
    pub name: Identifier,
    pub value: ValueExpression,
}

/// Destructure one value into names in the enclosing evaluation scope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bind {
    /// (public output name, local binding name).
    pub outputs: BTreeMap<Identifier, Identifier>,
    pub value: ValueExpression,
}

/// A let or bind declaration in the surrounding evaluation scope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocalBinding {
    Let(Let),
    Bind(Bind),
}

impl Let {
    pub(super) fn validate(&self, depth: usize) -> Result<(), Error> {
        self.value.validate(depth)
    }
}

impl Bind {
    pub(super) fn validate(&self, depth: usize) -> Result<(), Error> {
        self.value.validate(depth)?;
        if self.outputs.is_empty() {
            return Err(Error("bind requires at least one output binding".into()));
        }
        Ok(())
    }
}

impl LocalBinding {
    pub(crate) fn validate(&self, depth: usize) -> Result<(), Error> {
        match self {
            Self::Let(value) => value.validate(depth),
            Self::Bind(value) => value.validate(depth),
        }
    }
}
