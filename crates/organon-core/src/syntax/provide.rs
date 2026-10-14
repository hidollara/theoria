//! Expose an existing binding as an Element or interface output.
use crate::identifier::Identifier;

/// Supply a named output of an Element or an interface implementation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Provide {
    pub name: Identifier,
    pub reference: Identifier,
}
