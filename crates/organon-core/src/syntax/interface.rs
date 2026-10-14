//! Named output contracts and explicit implementations of those contracts.
use super::{Identifier, Port, Provide};
use crate::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Interface {
    pub name: Identifier,
    pub outputs: Vec<Port>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Implementation {
    pub interface: Identifier,
    pub provides: Vec<Provide>,
}

impl Interface {
    pub fn validate(&self) -> Result<(), Error> {
        super::validate_definition_name(&self.name, "interface")
    }
}
