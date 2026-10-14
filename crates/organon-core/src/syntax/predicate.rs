//! Named predicate definitions and their ordered members.
use super::{Binding, Identifier, Port, PredicateExpression};
use crate::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Predicate {
    pub name: Identifier,
    pub members: Vec<PredicateMember>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PredicateMember {
    Input(Port),
    Binding(Binding),
    Predicate(PredicateExpression),
}

impl Predicate {
    pub fn validate(&self) -> Result<(), Error> {
        super::validate_definition_name(&self.name, "predicate")?;
        let mut predicates = 0;
        for member in &self.members {
            match member {
                PredicateMember::Input(_) => (),
                PredicateMember::Binding(local) => local.validate(0)?,
                PredicateMember::Predicate(condition) => {
                    predicates += 1;
                    condition.validate(0)?;
                }
            }
        }
        if predicates != 1 {
            return Err(Error(
                "predicate requires exactly one predicate expression".into(),
            ));
        }
        Ok(())
    }
}
