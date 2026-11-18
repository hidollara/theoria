//! Representation and parsing of Aition source syntax.

/// Public syntax structures.
pub mod ast {
    pub use super::name::{Identifier, InvalidIdentifier, QualifiedName};
    pub use super::parser::node::{AstNode, Span};
}

mod name;
mod parser;

pub use parser::{
    Expected, Parsable, ParseError, Parsed, Parser, TokenStream, Trivia, TriviaKind, parse,
};
