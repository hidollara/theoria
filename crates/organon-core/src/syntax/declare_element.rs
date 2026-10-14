//! Input, output and conformance contracts for Elements implemented by a backend.
use super::{ChildrenType, Identifier, Port};
use crate::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeclareElement {
    pub name: Identifier,
    /// Whether this declaration permits tag construction.
    pub tag: bool,
    pub children: Option<ChildrenType>,
    pub members: Vec<DeclareElementMember>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeclareElementMember {
    Implements(Identifier),
    Input(Port),
    OptionalInput(Port),
    Output(Port),
}

impl DeclareElement {
    pub fn validate(&self) -> Result<(), Error> {
        super::validate_definition_name(&self.name, "element declaration")?;
        Ok(())
    }

    pub fn implements(&self) -> impl Iterator<Item = &str> {
        self.members.iter().filter_map(|member| match member {
            DeclareElementMember::Implements(n) => Some(n.as_str()),
            _ => None,
        })
    }
    pub fn inputs(&self) -> impl Iterator<Item = &Port> {
        self.members.iter().filter_map(|member| match member {
            DeclareElementMember::Input(v) | DeclareElementMember::OptionalInput(v) => Some(v),
            _ => None,
        })
    }
    pub fn outputs(&self) -> impl Iterator<Item = &Port> {
        self.members.iter().filter_map(|member| match member {
            DeclareElementMember::Output(v) => Some(v),
            _ => None,
        })
    }

    pub fn is_optional_input(&self, name: &str) -> bool {
        self.members.iter().any(
            |member| matches!(member, DeclareElementMember::OptionalInput(p) if p.name == name),
        )
    }
}
