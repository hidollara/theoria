//! Organon type vocabulary shared by authored declarations and semantic IR.
//! Element type names are syntax in stored data and resolved names inside a checked Program.
use crate::{Error, identifier};

/// A type name as written. Name resolution belongs to semantic analysis.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeName(identifier::Identifier);
impl TypeName {
    pub fn parse(value: &str) -> Result<Self, Error> {
        Ok(Self(value.parse()?))
    }
    pub fn spelling(&self) -> &str {
        &self.0
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Port<T = ValueType> {
    pub name: identifier::Identifier,
    pub ty: T,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueType {
    pub kind: TypeName,
    /// Complete set of reason types accepted or returned by a declared port.
    pub errors: Vec<TypeName>,
}

impl ValueType {
    pub fn fallible(&self) -> bool {
        !self.errors.is_empty()
    }

    pub fn spelling(&self) -> String {
        let value = self.kind.spelling();
        if self.fallible() {
            format!(
                "{value} (errors: {})",
                self.errors
                    .iter()
                    .map(TypeName::spelling)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        } else {
            value.into()
        }
    }
}

/// Ordered homogeneous children accepted by a construction contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChildrenType {
    pub kind: TypeName,
    pub minimum: usize,
    pub maximum: Option<usize>,
}
impl ChildrenType {
    pub fn validate(&self) -> Result<(), Error> {
        if self.maximum.is_some_and(|n| n < self.minimum) {
            return Err(Error("invalid child count range".into()));
        }
        Ok(())
    }
}
