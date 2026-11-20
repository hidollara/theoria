use super::error::ParseError;
use super::state::Parser;
use crate::syntax::ast::{AstNode, Identifier};

pub(super) fn identifier(parser: &mut Parser<'_, '_>) -> Result<AstNode<Identifier>, ParseError> {
    let start = parser.cursor()?;
    let Some(word) = parser.word()? else {
        return Err(ParseError::expected("an identifier", start));
    };
    let value =
        Identifier::new(word.value).map_err(|_| ParseError::expected("an identifier", start))?;
    Ok(AstNode {
        value,
        source: word.source,
    })
}
