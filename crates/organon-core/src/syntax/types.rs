//! Authored type annotations and child constraints, before name resolution.
use super::Identifier;
use crate::Error;

/// A type name as written. Name resolution belongs to semantic analysis.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeName(Identifier);
impl TypeName {
    pub fn parse(value: &str) -> Result<Self, Error> {
        Ok(Self(value.parse()?))
    }
    pub fn spelling(&self) -> &str {
        &self.0
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Port {
    pub name: Identifier,
    pub ty: TypeAnnotation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeAnnotation {
    pub kind: TypeName,
    /// Authored failure type names, in declaration order; not yet resolved or checked.
    pub errors: Vec<TypeName>,
}

/// Ordered homogeneous children accepted by a construction contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChildrenType {
    pub kind: TypeName,
    pub count: ChildCount,
}

/// Permitted child counts, independent of the child Element type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChildCount {
    One,
    Optional,
    Many,
}
