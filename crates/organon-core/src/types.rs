//! Organon type vocabulary shared by authored declarations and semantic IR.
//! Element type names are syntax in stored data and resolved names inside a checked Program.
use crate::{Error, names};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Input {
    pub name: String,
    pub kind: Type,
}

/// A type name as written. Name resolution belongs to semantic analysis.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Type(String);
impl Type {
    pub fn parse(value: &str) -> Result<Self, Error> {
        if names::identifier(value) {
            Ok(Self(value.into()))
        } else {
            Err(Error(format!("invalid type name {value:?}")))
        }
    }
    pub fn spelling(&self) -> &str {
        &self.0
    }
    pub fn validate(&self) -> Result<(), Error> {
        Self::parse(&self.0).map(|_| ())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Port {
    pub name: String,
    pub ty: ValueType,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueType {
    pub kind: Type,
    /// Whether obtaining the value can fail; its error representation belongs to the backend.
    pub fallible: bool,
}

impl ValueType {
    pub fn parse(value: &str, fallible: bool) -> Result<Self, Error> {
        let kind = Type::parse(value)?;
        Ok(Self { kind, fallible })
    }

    pub fn spelling(&self) -> String {
        let value = self.kind.spelling();
        if self.fallible {
            format!("Result<{value}>")
        } else {
            value.into()
        }
    }
}

/// Ordered homogeneous children accepted by a construction contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Children {
    pub kind: Type,
    pub minimum: usize,
    pub maximum: Option<usize>,
}
impl Children {
    pub fn validate(&self) -> Result<(), Error> {
        self.kind.validate()?;
        if self.maximum.is_some_and(|n| n < self.minimum) {
            return Err(Error("invalid child count range".into()));
        }
        Ok(())
    }
}
