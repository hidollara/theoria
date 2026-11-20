//! Parsing tokens into syntax, without name resolution or type checking.
//!
//! A terminal consumes one matching token via `Parser::next_if`. A `rule`
//! closure sequences reads in source order. `or`, `empty`, and `defer` provide
//! ordered alternatives, empty success, and references to other productions.
//! Repetition and delimited lists build on those operations.

mod choice;
mod closure;
mod expr;
mod label;
mod list;
mod located;
mod map;
mod module;
mod name;
mod primitive;
mod repeated;
mod rule;
mod string;
mod symbol;
mod trivia;
mod types;
mod word;

use super::lexer::{LexError, Token, TokenKind, TokenStream, TokenStreamItem};
use crate::{
    input::Cursor,
    syntax::ast::{AstNode, NodeSource},
};
use std::{error::Error, fmt};

pub(super) use self::choice::choice;
pub(super) use self::closure::rule;
use self::list::list;
use self::located::located;
pub(super) use self::module::module;
use self::primitive::{defer, empty, or};
pub(super) use self::repeated::repeated;
pub use self::rule::Rule;
pub(super) use self::string::string;
pub(super) use self::symbol::symbol;
pub(super) use self::trivia::trivia;
pub(super) use self::word::word;

/// Read one production and require it to account for the entire input.
pub(super) fn read_complete<'s, T>(
    mut tokens: impl TokenStream<'s>,
    rule: impl Rule<Output = (T, NodeSource)>,
) -> Result<AstNode<T>, ParseError> {
    let mut parser = Parser::new(&mut tokens);
    let node = parser.read(rule)?;
    if let TokenStreamItem::Token(next) = parser.peek()? {
        return Err(ParseError::expected("end of input".into(), next.start));
    }
    Ok(node.into())
}

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

    fn evaluate<T>(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<T, ParseError>,
    ) -> Result<Option<T>, ParseError> {
        let start = self.consumed;
        let result = f(self);
        if self.consumed == start && matches!(result, Err(ParseError::Unexpected { .. })) {
            return Ok(None);
        }
        result.map(Some)
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
