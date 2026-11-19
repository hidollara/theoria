//! Representation and parsing of Aition source syntax.

/// Public syntax structures.
pub mod ast {
    pub use super::binding::{Binding, BindingBody};
    pub use super::body::Body;
    pub use super::expr::Expr;
    pub use super::lambda::Lambda;
    pub use super::name::{Identifier, InvalidIdentifier, QualifiedName};
    pub use super::node::AstNode;
    pub use super::parameter::Parameter;
    pub use super::types::Type;
}

mod binding;
mod body;
mod expr;
mod lambda;
mod name;
mod node;
mod parameter;
mod parser;
mod token;
mod types;

use self::parser::{Parser, Syntax};
use crate::lexer::LexemeIterator;

pub use parser::ParseError;
pub use token::{Span, TokenStream, Trivia, TriviaKind};

/// Interpret a complete token stream using `T`'s grammar.
/// Return the syntax and collected trivia, in source order, as separate tuple entries.
/// Both success and failure consume the stream.
pub fn parse<'s, T: Syntax>(
    tokens: token::TokenStream<'s, impl LexemeIterator<'s>>,
) -> Result<(T, Vec<token::Trivia>), parser::ParseError> {
    let parser = T::parser();
    let (syntax, mut tokens) = parser.read(tokens)?;
    let Some(syntax) = syntax else {
        return Err(parser::ParseError::expected(
            parser.description(),
            tokens.peek()?.cursor(),
        ));
    };
    let trivia = parser::finish(tokens)?;
    Ok((syntax, trivia))
}
