//! Parsing tokens into syntax, without name resolution or type checking.

mod choice;
mod error;
mod identifier;
mod module;
mod repeated;
mod state;
mod trivia;
mod type_statement;

use super::lexer::TokenStream;
use crate::syntax::ast::{AstNode, Module};

use self::state::Parser;

pub use self::error::ParseError;

trait Rule: Sized {
    type Output;

    fn apply(&self, parser: &mut Parser<'_, '_>) -> Result<Self::Output, ParseError>;

    fn map<T>(self, convert: impl Fn(Self::Output) -> T) -> impl Rule<Output = T> {
        move |parser: &mut Parser<'_, '_>| self.apply(parser).map(&convert)
    }
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
    let module = module::body(&mut parser)?;
    if !parser.is_eoi()? {
        return Err(module::expected(parser.cursor()?));
    }
    Ok(module)
}
