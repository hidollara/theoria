//! Selection and failure-handling scopes, preserving member and handler order.
use super::{LocalBinding, PredicateExpression};
use crate::identifier::Identifier;
use crate::{Error, types::TypeName};

/// Attribute text is interpreted by the success type or, for a throwing exit, the reason type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Exit {
    pub result: String,
    pub throw: Option<TypeName>,
}

impl Exit {
    /// Select the type that interprets result text; this does not parse the text.
    pub fn parser_type<'a>(&'a self, success: &'a TypeName) -> &'a TypeName {
        self.throw.as_ref().unwrap_or(success)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BranchMember {
    Binding(LocalBinding),
    Predicate(PredicateExpression),
}

/// The shared contents of a case or catch: local bindings, an optional
/// predicate expression, and a result or throw. A catch adds its error filter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Branch {
    pub exit: Exit,
    pub members: Vec<BranchMember>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Catch {
    pub reason: Option<TypeName>,
    pub name: Option<Identifier>,
    pub branch: Branch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TryMember {
    Binding(LocalBinding),
    Catch(Catch),
    Unreachable { reason: Option<TypeName> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Try {
    /// The successful result type; catches may construct or throw different values.
    pub result_type: TypeName,
    pub result: String,
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
                    if handler.name.is_some() && handler.reason.is_none() {
                        return Err(Error("catch name requires a type".into()));
                    }
                    handler.branch.validate(depth)?;
                }
            }
        }
        Ok(())
    }
}
