use super::{
    parser::{ParseError, Parser, Syntax},
    token::{Span, TokenStream},
};
use crate::lexer::LexemeIterator;

/// A syntax node with an optional source span.
/// Parsed nodes have spans; programmatically constructed nodes need not have them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AstNode<T> {
    pub value: T,
    pub span: Option<Span>,
}

impl<T> AstNode<T> {
    pub fn new(value: T) -> Self {
        Self { value, span: None }
    }
}

impl<T> From<T> for AstNode<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

impl<T: Syntax> Syntax for AstNode<T> {
    fn parser() -> impl Parser<Output = Self> {
        AstNodeParser(T::parser())
    }
}

/// A parser that wraps its result in an AST node with the consumed source span.
struct AstNodeParser<P>(P);

impl<P: Parser> Parser for AstNodeParser<P> {
    type Output = AstNode<P::Output>;

    fn description(&self) -> String {
        self.0.description()
    }

    fn read<'s, I: LexemeIterator<'s>>(
        &self,
        mut tokens: TokenStream<'s, I>,
    ) -> Result<(Option<Self::Output>, TokenStream<'s, I>), ParseError> {
        let mark = tokens.mark()?;
        let (value, tokens) = self.0.read(tokens)?;
        let value = value.map(|value| AstNode {
            value,
            span: Some(tokens.span(&mark)),
        });
        Ok((value, tokens))
    }
}
