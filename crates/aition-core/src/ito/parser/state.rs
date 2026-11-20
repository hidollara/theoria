use super::error::ParseError;
use crate::input::Cursor;
use crate::ito::lexer::{Symbol, TokenKind, TokenStream, TokenStreamItem};
use crate::syntax::ast::AstNode;

enum TextKind {
    Word,
    Comment,
    Whitespace,
}

/// Reads a token stream and applies grammar rules at the current position.
pub(super) struct Parser<'s, 'input> {
    tokens: &'input mut dyn TokenStream<'s>,
    lookahead: Option<TokenStreamItem<'s>>,
    consumed: usize,
}

impl<'s, 'input> Parser<'s, 'input> {
    pub(super) fn new(tokens: &'input mut dyn TokenStream<'s>) -> Self {
        Self {
            tokens,
            lookahead: None,
            consumed: 0,
        }
    }

    /// Look ahead to the next token or the end of input.
    fn peek(&mut self) -> Result<&mut TokenStreamItem<'s>, ParseError> {
        if self.lookahead.is_none() {
            self.lookahead = Some(self.tokens.next()?);
        }
        Ok(self.lookahead.as_mut().unwrap())
    }

    pub(super) fn is_eoi(&mut self) -> Result<bool, ParseError> {
        Ok(matches!(self.peek()?, TokenStreamItem::Eoi(_)))
    }

    pub(super) fn cursor(&mut self) -> Result<Cursor, ParseError> {
        Ok(match self.peek()? {
            TokenStreamItem::Token(token) => token.start,
            TokenStreamItem::Eoi(end) => *end,
        })
    }

    pub(super) fn consumed(&self) -> usize {
        self.consumed
    }

    pub(super) fn keyword(&mut self, expected: &str) -> Result<Option<AstNode<()>>, ParseError> {
        self.terminal(TokenKind::Word(expected.into()))
    }

    pub(super) fn symbol(&mut self, expected: Symbol) -> Result<Option<AstNode<()>>, ParseError> {
        self.terminal(TokenKind::Symbol(expected))
    }

    pub(super) fn word(&mut self) -> Result<Option<AstNode<Box<str>>>, ParseError> {
        self.text(TextKind::Word)
    }

    pub(super) fn comment(&mut self) -> Result<Option<AstNode<Box<str>>>, ParseError> {
        self.text(TextKind::Comment)
    }

    pub(super) fn whitespace(&mut self) -> Result<Option<AstNode<Box<str>>>, ParseError> {
        self.text(TextKind::Whitespace)
    }

    fn text(&mut self, expected: TextKind) -> Result<Option<AstNode<Box<str>>>, ParseError> {
        let TokenStreamItem::Token(token) = self.peek()? else {
            return Ok(None);
        };
        let text = match (expected, &mut token.kind) {
            (TextKind::Word, TokenKind::Word(text))
            | (TextKind::Comment, TokenKind::LineComment(text))
            | (TextKind::Whitespace, TokenKind::Whitespace(text)) => std::mem::take(text).into(),
            _ => return Ok(None),
        };
        let node = AstNode::with_source_range(text, token.start, token.end);
        self.lookahead = None;
        self.consumed += 1;
        Ok(Some(node))
    }

    fn terminal(&mut self, expected: TokenKind<'_>) -> Result<Option<AstNode<()>>, ParseError> {
        let TokenStreamItem::Token(token) = self.peek()? else {
            return Ok(None);
        };
        if token.kind != expected {
            return Ok(None);
        }
        let node = AstNode::with_source_range((), token.start, token.end);
        self.lookahead = None;
        self.consumed += 1;
        Ok(Some(node))
    }
}

#[cfg(test)]
mod tests {
    use super::Parser;
    use crate::ito::lexer::{self, LexError, Symbol, TokenStream, TokenStreamItem};
    use std::cell::Cell;

    impl<'source, F> TokenStream<'source> for F
    where
        F: FnMut() -> Result<TokenStreamItem<'source>, LexError>,
    {
        fn next(&mut self) -> Result<TokenStreamItem<'source>, LexError> {
            self()
        }
    }

    #[test]
    fn text_tokens_keep_their_spelling_and_source_range() {
        let mut tokens = lexer::from_str("name// 幅\r\n");
        let mut parser = Parser::new(&mut tokens);

        assert!(parser.comment().unwrap().is_none());
        let word = parser.word().unwrap().unwrap();
        assert_eq!(word.value.as_ref(), "name");
        let source = word.source.unwrap();
        assert_eq!(source.start.offset..source.end.offset, 0..4);

        assert!(parser.whitespace().unwrap().is_none());
        let comment = parser.comment().unwrap().unwrap();
        assert_eq!(comment.value.as_ref(), " 幅");
        let source = comment.source.unwrap();
        assert_eq!(source.start.offset..source.end.offset, 4..10);

        let space = parser.whitespace().unwrap().unwrap();
        assert_eq!(space.value.as_ref(), "\r\n");
        let source = space.source.unwrap();
        assert_eq!(source.start.offset..source.end.offset, 10..12);
        assert!(parser.word().unwrap().is_none());
        assert_eq!(parser.consumed(), 3);
    }

    #[test]
    fn keyword_and_symbol_cover_exactly_the_matched_tokens() {
        let mut tokens = lexer::from_str("typeName(");
        let mut parser = Parser::new(&mut tokens);

        assert!(parser.keyword("type").unwrap().is_none());
        let word = parser.keyword("typeName").unwrap().unwrap().source.unwrap();
        assert_eq!(word.start.offset..word.end.offset, 0..8);

        assert!(parser.symbol(Symbol::CloseParen).unwrap().is_none());
        let open = parser
            .symbol(Symbol::OpenParen)
            .unwrap()
            .unwrap()
            .source
            .unwrap();
        assert_eq!(open.start.offset..open.end.offset, 8..9);
        assert!(parser.symbol(Symbol::CloseParen).unwrap().is_none());
        assert_eq!(parser.consumed(), 2);
    }

    #[test]
    fn lookahead_reads_each_token_and_eoi_only_once() {
        let reads = Cell::new(0);
        let mut tokens = lexer::from_str(" ");
        let mut stream = || {
            reads.set(reads.get() + 1);
            tokens.next()
        };
        let mut parser = Parser::new(&mut stream);
        assert_eq!(parser.cursor().unwrap().offset, 0);
        assert!(parser.whitespace().unwrap().is_some());
        assert_eq!(reads.get(), 1);

        assert!(parser.is_eoi().unwrap());
        assert!(parser.whitespace().unwrap().is_none());
        assert_eq!(parser.cursor().unwrap().offset, 1);
        assert_eq!(reads.get(), 2);
    }
}
