//! Value expressions and predicate expressions, before name or type resolution.
use super::{Select, Try};
use crate::identifier::Identifier;
use crate::{Error, types::TypeName};
use std::collections::BTreeMap;

/// A value-producing expression, or a children splice where a sequence is accepted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValueExpression {
    Select(Select),
    Try(Try),
    Reference {
        name: Identifier,
        ty: Option<TypeName>,
    },
    Construct(Construction),
    /// Insert the enclosing element's incoming children, preserving their order.
    Children,
}

/// A request to construct an Element, not an already constructed runtime value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Construction {
    pub element: Identifier,
    /// Argument text, interpreted after the receiving types are resolved.
    pub arguments: BTreeMap<Identifier, String>,
    pub children: Vec<ValueExpression>,
}

impl ValueExpression {
    pub(crate) fn validate(&self, depth: usize) -> Result<(), Error> {
        super::check_depth(depth)?;
        match self {
            Self::Select(select) => select.validate(depth + 1),
            Self::Try(value) => value.validate(depth + 1),
            Self::Children | Self::Reference { .. } => Ok(()),
            Self::Construct(value) => {
                if crate::syntax::reserved_name(&value.element) {
                    return Err(Error("construction requires a valid element name".into()));
                }
                for child in &value.children {
                    child.validate(depth + 1)?;
                }
                Ok(())
            }
        }
    }
}

/// A predicate call or logical combination, distinct from a named Predicate definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PredicateExpression {
    Call {
        name: Identifier,
        arguments: BTreeMap<Identifier, String>,
    },
    All(Vec<PredicateExpression>),
    Any(Vec<PredicateExpression>),
    Not(Box<PredicateExpression>),
}

impl PredicateExpression {
    pub(crate) fn validate(&self, depth: usize) -> Result<(), Error> {
        super::check_depth(depth)?;
        match self {
            Self::Call { name, .. } => super::validate_definition_name(name, "predicate"),
            Self::All(items) | Self::Any(items) => {
                items.iter().try_for_each(|p| p.validate(depth + 1))
            }
            Self::Not(p) => p.validate(depth + 1),
        }
    }
}
