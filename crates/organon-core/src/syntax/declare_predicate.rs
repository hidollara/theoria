//! Input and failure contracts for predicates implemented by a backend.
use crate::identifier::Identifier;
use crate::{
    Error,
    types::{Port, TypeName},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeclarePredicate {
    pub name: Identifier,
    pub members: Vec<DeclarePredicateMember>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeclarePredicateMember {
    Input(Port),
    Error(TypeName),
}

impl DeclarePredicate {
    pub fn validate(&self) -> Result<(), Error> {
        super::validate_definition_name(&self.name, "predicate declaration")
    }

    pub fn inputs(&self) -> impl Iterator<Item = &Port> {
        self.members.iter().filter_map(|member| match member {
            DeclarePredicateMember::Input(i) => Some(i),
            _ => None,
        })
    }
    pub fn errors(&self) -> impl Iterator<Item = &TypeName> {
        self.members.iter().filter_map(|member| match member {
            DeclarePredicateMember::Error(e) => Some(e),
            _ => None,
        })
    }
}
