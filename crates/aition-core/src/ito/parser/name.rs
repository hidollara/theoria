use super::super::lexer::Symbol;
use super::{Rule, defer, located, repeated, rule, symbol, trivia, word};
use crate::syntax::ast::{Identifier, NodeSource, QualifiedName};

pub(super) fn identifier() -> impl Rule<Output = (Identifier, NodeSource)> {
    located(
        word("an identifier").try_map("an identifier", |(text, end)| {
            Identifier::new(text)
                .ok()
                .map(|name| (name, Vec::new(), end))
        }),
    )
}

pub(super) fn qualified_name() -> impl Rule<Output = (QualifiedName, NodeSource)> {
    let identifier = defer(identifier);
    rule("an identifier", move |parser| {
        let start = parser.cursor()?;
        let head = parser.read(&identifier)?;
        let mut end = head.1.end;
        let mut tail = Vec::new();
        let mut gaps = Vec::new();
        loop {
            gaps.extend(parser.read(repeated(trivia()))?);
            if symbol(Symbol::Dot).read(parser)?.is_none() {
                return Ok((
                    QualifiedName::new(head.into(), tail),
                    NodeSource {
                        start,
                        end,
                        trivia: gaps,
                    },
                ));
            }
            gaps.extend(parser.read(repeated(trivia()))?);
            let part = parser.read(&identifier)?;
            end = part.1.end;
            tail.push(part.into());
        }
    })
}
