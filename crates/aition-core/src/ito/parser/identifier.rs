use super::error::ParseError;
use super::rule::{Rule, repeated, sequence, symbol, word};
use crate::ito::lexer::Symbol;
use crate::syntax::{Identifier, QualifiedName};

pub(super) fn identifier() -> Rule<Identifier> {
    word().expecting("an identifier").try_map(|(text, cursor)| {
        Identifier::new(text)
            .map(|name| name.with_cursor(cursor))
            .map_err(|error| ParseError::expected(error.expected, cursor))
    })
}

pub(super) fn qualified_name() -> Rule<QualifiedName> {
    let name = sequence![
        identifier(),
        repeated(sequence![symbol(Symbol::Dot), identifier()].map(|(_, part)| part)),
    ];
    name.map(|(head, tail)| QualifiedName::new(head, tail))
}
