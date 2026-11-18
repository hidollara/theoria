use crate::lexer::{self, Cursor, LexError, LexemeIterator, Symbol};
use std::{borrow::Cow, ops::Range};

/// Turns lexemes into grammar tokens with lookahead, collecting trivia separately.
/// Word spelling and grammar are left to the parser.
pub struct TokenStream<'source, I> {
    lexemes: I,
    /// End of the last read lexeme, including lookahead, whitespace, and comments.
    read_end: Cursor,
    lookahead: Option<Lookahead<'source>>,
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

    /// Return trivia collected so far, without reading or validating the remainder.
    pub(super) fn into_trivia(self) -> Vec<Trivia> {
        self.trivia
    }

    /// Read ahead to a token or EOI, retaining its position and collecting skipped trivia once.
    pub(super) fn peek(&mut self) -> Result<&Lookahead<'source>, LexError> {
        while self.lookahead.is_none() {
            let Some(lexeme) = self.lexemes.next() else {
                self.lookahead = Some(Lookahead::Eoi(self.read_end));
                break;
            };
            let lexeme = lexeme?;
            self.read_end = lexeme.end;
            let kind = match lexeme.kind {
                lexer::LexemeKind::Word(text) => TokenKind::Word(text),
                lexer::LexemeKind::String(text) => TokenKind::String(text),
                lexer::LexemeKind::Symbol(symbol) => TokenKind::Symbol(symbol),
                lexer::LexemeKind::Whitespace(text) => {
                    self.trivia.push(Trivia {
                        kind: TriviaKind::Whitespace(text.into()),
                        start: lexeme.start,
                        end: lexeme.end,
                    });
                    continue;
                }
                lexer::LexemeKind::LineComment(text) => {
                    self.trivia.push(Trivia {
                        kind: TriviaKind::LineComment(text.into()),
                        start: lexeme.start,
                        end: lexeme.end,
                    });
                    continue;
                }
            };
            self.lookahead = Some(Lookahead::Token(Token {
                kind,
                start: lexeme.start,
                end: lexeme.end,
            }));
        }
        Ok(self.lookahead.as_ref().unwrap())
    }

    /// Consume and return the next token only if its kind matches the predicate.
    /// A mismatch or EOF returns `None` without consuming a token.
    pub(super) fn next_if(
        &mut self,
        matches: impl FnOnce(&TokenKind<'source>) -> bool,
    ) -> Result<Option<Token<'source>>, LexError> {
        self.peek()?;
        let Some(Lookahead::Token(token)) = self
            .lookahead
            .take_if(|next| matches!(next, Lookahead::Token(token) if matches(&token.kind)))
        else {
            return Ok(None);
        };
        self.consumed += 1;
        self.consumed_end = token.span().end;
        Ok(Some(token))
    }

    pub(super) fn mark(&mut self) -> Result<Mark, LexError> {
        let start = self.peek()?.cursor();
        Ok(Mark {
            start,
            consumed: self.consumed,
        })
    }

    pub(super) fn span(&self, mark: &Mark) -> Span {
        let end = if self.consumed == mark.consumed {
            mark.start
        } else {
            self.consumed_end
        };
        Span {
            start: mark.start,
            end,
        }
    }
}

/// A completed lookahead, including the position when there is no next token.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Lookahead<'source> {
    Token(Token<'source>),
    Eoi(Cursor),
}

impl Lookahead<'_> {
    pub fn cursor(&self) -> Cursor {
        match self {
            Self::Token(token) => token.start,
            Self::Eoi(cursor) => *cursor,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct Mark {
    start: Cursor,
    consumed: usize,
}

/// A half-open range of source positions, retaining byte offsets and line/column pairs.
/// Leading and trailing trivia are excluded from a parsed node's span.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: Cursor,
    /// Position just after the span.
    pub end: Cursor,
}

impl Span {
    /// UTF-8 byte range for indexing the source text.
    pub fn range(&self) -> Range<usize> {
        self.start.offset..self.end.offset
    }
}

/// A token available to grammar readers, excluding trivia by construction.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Token<'source> {
    pub kind: TokenKind<'source>,
    pub start: Cursor,
    pub end: Cursor,
}

impl Token<'_> {
    pub fn span(&self) -> Span {
        Span {
            start: self.start,
            end: self.end,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum TokenKind<'source> {
    Word(Cow<'source, str>),
    String(Cow<'source, str>),
    Symbol(Symbol),
}

/// Whitespace or a comment retained independently of syntax nodes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trivia {
    pub kind: TriviaKind,
    /// Start of the whitespace or comment, including the comment delimiter.
    pub start: Cursor,
    /// Position just after the trivia. A comment's line ending is separate whitespace.
    pub end: Cursor,
}

impl Trivia {
    pub fn span(&self) -> Span {
        Span {
            start: self.start,
            end: self.end,
        }
    }
}

/// Trivia text owned by the parse result, independent of the input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TriviaKind {
    Whitespace(Box<str>),
    /// The body after `//`, preserving spaces and excluding the line ending.
    LineComment(Box<str>),
}

#[cfg(test)]
mod tests;
