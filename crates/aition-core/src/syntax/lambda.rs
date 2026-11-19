use super::{
    body::Body,
    node::AstNode,
    parameter::Parameter,
    parser::{Parser, Syntax, list, sequence},
};
use crate::lexer::Symbol;

/// Parameters followed by a body: `[parameters] (expression) { ... }`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lambda {
    /// Source order is retained. An empty list denotes a function with no parameters.
    pub parameters: Vec<AstNode<Parameter>>,
    /// Parameters are available to both the result and its helper bindings.
    pub body: AstNode<Body>,
}

impl Syntax for Lambda {
    fn parser() -> impl Parser<Output = Self> {
        sequence![
            list(
                AstNode::<Parameter>::parser(),
                Symbol::OpenBracket,
                Symbol::CloseBracket,
                Symbol::Comma,
                true,
            ),
            AstNode::<Body>::parser(),
        ]
        .map(|(parameters, body)| Self { parameters, body })
    }
}
