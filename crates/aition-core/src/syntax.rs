//! Representation and parsing of Aition source syntax.

/// Public syntax structures.
pub mod ast {
    pub use super::binding::{Binding, BindingBody};
    pub use super::body::Body;
    pub use super::expr::Expr;
    pub use super::lambda::Lambda;
    pub use super::literal::Literal;
    pub use super::name::{Identifier, InvalidIdentifier, QualifiedName};
    pub use super::parameter::Parameter;
    pub use super::parser::node::{AstNode, Span};
    pub use super::tag::{Tag, TagArgument, TagArgumentValue, TagChild};
    pub use super::types::Type;
}

mod binding;
mod body;
mod expr;
mod lambda;
mod literal;
mod name;
mod parameter;
mod parser;
mod tag;
mod types;

pub use parser::{
    Expected, Parsable, ParseError, Parsed, Parser, TokenStream, Trivia, TriviaKind, parse,
};
