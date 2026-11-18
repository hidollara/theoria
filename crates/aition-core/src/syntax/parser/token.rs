use std::{borrow::Cow, ops::Range};

use crate::lexer::{self, Cursor, LexemeIterator, Symbol};

use super::{Expected, ParseError, Trivia, TriviaKind, node::Span};

/// Turns lexemes into grammar tokens with lookahead, collecting trivia separately.
/// Word spelling and grammar are left to the parser.
pub struct TokenStream<'source, I> {
    lexemes: I,
    /// End of the last read lexeme, including lookahead, whitespace, and comments.
    read_end: Cursor,
    lookahead: Option<Token<'source>>,
    trivia: Vec<Trivia>,
    consumed: usize,
    consumed_end: Cursor,
}

impl<'source, I: LexemeIterator<'source>> TokenStream<'source, I> {
    pub fn new(lexemes: I) -> Self {
        Self {
            lexemes,
            read_end: Cursor::default(),
            lookahead: None,
            trivia: Vec::new(),
            consumed: 0,
            consumed_end: Cursor::default(),
        }
    }

    /// Require EOF after trailing trivia and return the collected trivia.
    /// Errors from the lexeme iterator are propagated as lexical errors.
    pub(in crate::syntax) fn finish(mut self) -> Result<Vec<Trivia>, ParseError> {
        if self.peek()?.is_some() {
            return Err(self.error(Expected::EndOfInput));
        }
        Ok(self.trivia)
    }

    /// Grammar readers see tokens only; skipped trivia is recorded once.
    pub(in crate::syntax) fn peek(&mut self) -> Result<Option<&Token<'source>>, ParseError> {
        while self.lookahead.is_none() {
            let Some(lexeme) = self.lexemes.next() else {
                break;
            };
            let lexeme = lexeme.map_err(ParseError::Lexical)?;
            self.read_end = lexeme.end;
            let span = lexeme.span();
            let kind = match lexeme.kind {
                lexer::LexemeKind::Word(text) => TokenKind::Word(text),
                lexer::LexemeKind::String(text) => TokenKind::String(text),
                lexer::LexemeKind::Symbol(symbol) => TokenKind::Symbol(symbol),
                lexer::LexemeKind::Whitespace(text) => {
                    self.trivia.push(Trivia {
                        kind: TriviaKind::Whitespace(text.into()),
                        span,
                        cursor: lexeme.start,
                    });
                    continue;
                }
                lexer::LexemeKind::LineComment(text) => {
                    self.trivia.push(Trivia {
                        kind: TriviaKind::LineComment(text.into()),
                        span,
                        cursor: lexeme.start,
                    });
                    continue;
                }
            };
            self.lookahead = Some(Token {
                kind,
                start: lexeme.start,
                end: lexeme.end,
            });
        }
        Ok(self.lookahead.as_ref())
    }

    /// Consume the next significant token after any intervening trivia.
    pub(in crate::syntax) fn next(&mut self) -> Result<Option<Token<'source>>, ParseError> {
        self.peek()?;
        let token = self.lookahead.take();
        if let Some(token) = &token {
            self.consumed += 1;
            self.consumed_end = token.end;
        }
        Ok(token)
    }

    pub(in crate::syntax) fn eat(&mut self, symbol: Symbol) -> Result<bool, ParseError> {
        if self
            .peek()?
            .is_some_and(|token| matches!(token.kind, TokenKind::Symbol(found) if found == symbol))
        {
            self.next()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub(in crate::syntax) fn error(&mut self, expected: Expected) -> ParseError {
        match self.peek() {
            Ok(token) => {
                let token = token.map(|token| (token.span(), token.start));
                let (span, cursor) =
                    token.unwrap_or((self.read_end.offset..self.read_end.offset, self.read_end));
                ParseError::Unexpected {
                    span,
                    cursor,
                    expected,
                }
            }
            Err(error) => error,
        }
    }

    pub(super) fn mark(&mut self) -> Result<Mark, ParseError> {
        let start = self
            .peek()?
            .map(|token| token.start)
            .unwrap_or(self.read_end);
        Ok(Mark {
            start,
            consumed: self.consumed,
        })
    }

    pub(super) fn span(&self, mark: &Mark) -> Span {
        let end = if self.consumed == mark.consumed {
            mark.start.offset
        } else {
            self.consumed_end.offset
        };
        Span {
            range: mark.start.offset..end,
            start: mark.start,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct Mark {
    start: Cursor,
    consumed: usize,
}

/// A token available to grammar readers, excluding trivia by construction.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::syntax) struct Token<'source> {
    pub kind: TokenKind<'source>,
    pub start: Cursor,
    pub end: Cursor,
}

impl Token<'_> {
    pub fn span(&self) -> Range<usize> {
        self.start.offset..self.end.offset
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(in crate::syntax) enum TokenKind<'source> {
    Word(Cow<'source, str>),
    String(Cow<'source, str>),
    Symbol(Symbol),
}

#[cfg(test)]
mod tests;
