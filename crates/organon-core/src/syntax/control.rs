//! Selection and failure-handling scopes, preserving member and handler order.
use super::{Binding, Identifier, PredicateExpression, TypeName};
use crate::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BranchMember {
    Binding(Binding),
    Predicate(PredicateExpression),
}

/// The shared contents of a case or catch: local bindings, an optional
/// predicate expression, and a result or throw. A catch adds its error filter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Branch {
    /// Attribute text interpreted by the enclosing success type or the `throw` type.
    pub value: String,
    pub throw: Option<TypeName>,
    pub members: Vec<BranchMember>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Catch {
    /// The incoming error filter, independent of the branch's returned value.
    pub error: Option<TypeName>,
    pub capture: Option<Identifier>,
    pub branch: Branch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TryMember {
    Binding(Binding),
    Catch(Catch),
    Unreachable { error: Option<TypeName> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Try {
    /// The successful result type; catches may construct or throw different values.
    pub result_type: TypeName,
    pub value: String,
    pub members: Vec<TryMember>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Select {
    /// The result type shared by all non-throwing cases.
    pub result_type: TypeName,
    pub cases: Vec<Branch>,
}

impl Select {
    pub(crate) fn validate(&self, depth: usize) -> Result<(), Error> {
        self.cases.iter().try_for_each(|case| case.validate(depth))
    }
}

impl Branch {
    pub(crate) fn validate(&self, depth: usize) -> Result<(), Error> {
        let mut guarded = false;
        for member in &self.members {
            match member {
                BranchMember::Binding(local) => local.validate(depth)?,
                BranchMember::Predicate(condition) => {
                    if guarded {
                        return Err(Error(
                            "expected at most one predicate; use all or any to combine conditions"
                                .into(),
                        ));
                    }
                    condition.validate(depth)?;
                    guarded = true;
                }
            }
        }
        Ok(())
    }
}

impl Try {
    pub(crate) fn validate(&self, depth: usize) -> Result<(), Error> {
        for member in &self.members {
            match member {
                TryMember::Binding(local) => local.validate(depth)?,
                TryMember::Unreachable { .. } => (),
                TryMember::Catch(handler) => {
                    if handler.capture.is_some() && handler.error.is_none() {
                        return Err(Error("catch capture requires an error type".into()));
                    }
                    handler.branch.validate(depth)?;
                }
            }
        }
        Ok(())
    }
}
