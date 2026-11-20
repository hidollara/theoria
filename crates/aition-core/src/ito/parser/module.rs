use super::{Rule, repeated, rule, trivia};
use crate::syntax::ast::{Module, NodeSource};

pub fn module() -> impl Rule<Output = (Module, NodeSource)> {
    rule("a module", |parser| {
        let trivia = parser.read(repeated(trivia()))?;
        let end = parser.cursor()?;
        Ok((
            Module,
            NodeSource {
                start: end,
                end,
                trivia,
            },
        ))
    })
}
