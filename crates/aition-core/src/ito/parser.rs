//! Parsing tokens into syntax, without name resolution or type checking.

mod error;
mod repeated;
mod state;
mod trivia;

use super::lexer::TokenStream;
use crate::syntax::ast::{AstNode, Module, NodeSource};

use self::repeated::repeated;
use self::state::Parser;
use self::trivia::trivia;

pub use self::error::ParseError;

trait Rule {
    type Output;

    fn apply(&self, parser: &mut Parser<'_, '_>) -> Result<Self::Output, ParseError>;
}

impl<T, F> Rule for F
where
    F: Fn(&mut Parser<'_, '_>) -> Result<T, ParseError>,
{
    type Output = T;

    fn apply(&self, parser: &mut Parser<'_, '_>) -> Result<T, ParseError> {
        self(parser)
    }
}

pub(super) fn parse<'s>(mut tokens: impl TokenStream<'s>) -> Result<AstNode<Module>, ParseError> {
    let mut parser = Parser::new(&mut tokens);
    let start = parser.cursor()?;
    let trivia = repeated(trivia).apply(&mut parser)?;
    let end = parser.cursor()?;
    if !parser.is_eoi()? {
        return Err(ParseError::expected("end of input", end));
    }
    Ok(AstNode {
        value: Module,
        source: Some(NodeSource { start, end, trivia }),
    })
}
