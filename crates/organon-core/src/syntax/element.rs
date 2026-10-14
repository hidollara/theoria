//! Named Element definitions and their ordered members.
use super::{Binding, ChildrenType, Identifier, Implementation, Port, Provide};
use crate::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Element {
    pub name: Identifier,
    pub children: Option<ChildrenType>,
    pub members: Vec<ElementMember>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ElementMember {
    Impl(Implementation),
    Input(Port),
    Binding(Binding),
    Provide(Provide),
}

impl Element {
    pub fn implementations(&self) -> impl Iterator<Item = &Implementation> {
        self.members.iter().filter_map(|member| match member {
            ElementMember::Impl(v) => Some(v),
            _ => None,
        })
    }

    pub fn inputs(&self) -> impl Iterator<Item = &Port> {
        self.members.iter().filter_map(|member| match member {
            ElementMember::Input(v) => Some(v),
            _ => None,
        })
    }
    pub fn provides(&self) -> impl Iterator<Item = &Provide> {
        self.members.iter().filter_map(|member| match member {
            ElementMember::Provide(v) => Some(v),
            _ => None,
        })
    }
    /// Check definition structure, without resolving names or choosing a representation.
    pub fn validate(&self) -> Result<(), Error> {
        super::validate_definition_name(&self.name, "element")?;
        for member in &self.members {
            match member {
                ElementMember::Binding(binding) => binding.validate(0)?,
                ElementMember::Input(_) | ElementMember::Provide(_) | ElementMember::Impl(_) => (),
            }
        }
        Ok(())
    }
}
