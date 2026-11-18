//! Parsing tokens into syntax, without name resolution or type checking.

mod repeated;
mod rule;
mod trivia;

use super::lexer::{LexError, Token, TokenKind, TokenStream, TokenStreamItem};
use crate::input::Cursor;
use std::{error::Error, fmt};

pub(super) use self::repeated::repeated;
pub use self::rule::Rule;
pub(super) use self::trivia::trivia;

/// Reads a token stream and applies grammar rules at the current position.
pub struct Parser<'s, 'input> {
    tokens: &'input mut dyn TokenStream<'s>,
    lookahead: Option<TokenStreamItem<'s>>,
    consumed: usize,
}

impl<'s, 'input> Parser<'s, 'input> {
    pub fn new(tokens: &'input mut dyn TokenStream<'s>) -> Self {
        Self {
            tokens,
            lookahead: None,
            consumed: 0,
        }
    }

    /// Look ahead to the next token or the end of input.
    pub(super) fn peek(&mut self) -> Result<&TokenStreamItem<'s>, ParseError> {
        if self.lookahead.is_none() {
            self.lookahead = Some(self.tokens.next()?);
        }
        Ok(self.lookahead.as_ref().unwrap())
    }

    fn cursor(&mut self) -> Result<Cursor, ParseError> {
        Ok(match self.peek()? {
            TokenStreamItem::Token(token) => token.start,
            TokenStreamItem::Eoi(end) => *end,
        })
    }

    /// Read a required construct at the current token.
    pub fn read<R: Rule>(&mut self, rule: R) -> Result<R::Output, ParseError> {
        match rule.read(self)? {
            Some(value) => Ok(value),
            None => Err(ParseError::expected(rule.description(), self.cursor()?)),
        }
    }

    pub fn consumed(&self) -> usize {
        self.consumed
    }

    /// Consume a matching input token. Eoi marks a boundary and never advances the parser.
    pub fn next_if(
        &mut self,
        matches: impl FnOnce(&TokenKind<'s>) -> bool,
    ) -> Result<Option<Token<'s>>, ParseError> {
        self.peek()?;
        let item = self
            .lookahead
            .take_if(|item| matches!(item, TokenStreamItem::Token(token) if matches(&token.kind)));
        if let Some(TokenStreamItem::Token(token)) = item {
            self.consumed += 1;
            Ok(Some(token))
        } else {
            Ok(None)
        }
    }
}

/// An input, lexical, or syntax error while parsing.
#[derive(Debug)]
pub enum ParseError {
    Lexical(LexError),
    Unexpected {
        /// A diagnostic description of the syntax expected at this position.
        expected: String,
        cursor: Cursor,
    },
}

impl ParseError {
    /// Report a grammar expectation at the position selected by the parser.
    pub(super) fn expected(expected: String, cursor: Cursor) -> Self {
        Self::Unexpected { expected, cursor }
    }

    /// Source position of the diagnostic.
    pub fn cursor(&self) -> &Cursor {
        match self {
            Self::Lexical(error) => error.cursor(),
            Self::Unexpected { cursor, .. } => cursor,
        }
    }
}

impl From<LexError> for ParseError {
    fn from(error: LexError) -> Self {
        Self::Lexical(error)
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (expected, cursor) = match self {
            Self::Lexical(error) => return fmt::Display::fmt(error, formatter),
            Self::Unexpected {
                expected, cursor, ..
            } => (expected, cursor),
        };
        write!(
            formatter,
            "expected {expected} at line {}, column {}",
            cursor.line, cursor.column
        )
    }
}

impl Error for ParseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Lexical(error) => Some(error),
            Self::Unexpected { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod input_tests;
