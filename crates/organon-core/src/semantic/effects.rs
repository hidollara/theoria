//! Failure requirements shared by core validation and backend type generation.
//! Native attribute dependencies are filled by their parsers; the same graph
//! evaluates the known lower bound and the final sets.
use super::*;
mod evaluate;
mod graph;
pub use evaluate::{Node, evaluate};
pub use graph::{Check, Effect, Graph, Requirement, graph};

pub(crate) fn validate_known(program: &Program) -> Result<(), Error> {
    let graph = graph(program);
    let selected = graph
        .attributes
        .iter()
        .map(|a| vec![false; a.bindings.len()])
        .collect::<Vec<_>>();
    let sets = graph.evaluate(&selected);
    for check in &graph.checks {
        if sets[check.source]
            .iter()
            .enumerate()
            .any(|(i, present)| *present && !check.allowed.contains(&i))
        {
            return Err(Error(format!(
                "{}: failure outside the interface contract",
                check.name
            )));
        }
    }
    Ok(())
}
