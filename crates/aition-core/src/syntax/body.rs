use super::{
    binding::Binding,
    expr::Expr,
    node::AstNode,
    parser::{ParseError, Parser, Syntax, optional, repeated, sequence, symbol},
    token::TokenStream,
};
use crate::lexer::{LexemeIterator, Symbol};

/// A result expression and its optional helper definitions:
/// `(expression)` or `(expression) { name = other; ... }`.
/// Each explicit pair of grouping parentheses is retained.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Body {
    pub result: Box<AstNode<Expr>>,
    /// Local bindings, not statements executed after evaluating the result.
    /// `None` means no braces; `Some` retains even an empty `{}`.
    pub bindings: Option<Vec<AstNode<Binding>>>,
}

impl Syntax for Body {
    fn parser() -> impl Parser<Output = Self> {
        BodyParser
    }
}

struct BodyParser;

impl Parser for BodyParser {
    type Output = Body;

    fn description(&self) -> String {
        "'('".into()
    }

    fn read<'s, I: LexemeIterator<'s>>(
        &self,
        tokens: TokenStream<'s, I>,
    ) -> Result<(Option<Self::Output>, TokenStream<'s, I>), ParseError> {
        sequence![
            symbol(Symbol::OpenParen),
            AstNode::<Expr>::parser(),
            symbol(Symbol::CloseParen),
            optional(
                sequence![
                    symbol(Symbol::OpenBrace),
                    repeated(AstNode::<Binding>::parser()),
                    symbol(Symbol::CloseBrace),
                ]
                .map(|((), bindings, ())| bindings),
            ),
        ]
        .map(|((), result, (), bindings)| Body {
            result: Box::new(result),
            bindings,
        })
        .read(tokens)
    }
}
