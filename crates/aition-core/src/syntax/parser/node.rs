use std::ops::Range;

use crate::lexer::{Cursor, LexemeIterator};

use super::{Expected, Parsable, ParseError, Parser, token::TokenStream};

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

impl<T: Parsable> Parsable for AstNode<T> {
    fn parser() -> impl Parser<Output = Self> {
        AstNodeParser(T::parser())
    }
}

/// A UTF-8 byte range and its starting character position in the source.
/// Leading and trailing trivia are excluded from a parsed node's span.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub range: Range<usize>,
    pub start: Cursor,
}

/// A parser that wraps its result in an AST node with the consumed source span.
pub(in crate::syntax) struct AstNodeParser<P>(pub(in crate::syntax) P);

impl<P: Parser> Parser for AstNodeParser<P> {
    type Output = AstNode<P::Output>;
    fn expected(&self) -> Expected {
        self.0.expected()
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
