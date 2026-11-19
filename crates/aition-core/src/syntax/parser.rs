//! Parsing lexemes into syntax, without name resolution or type checking.

mod choice;
mod list;
mod map;
mod optional;
mod repeated;
mod sequence;
mod string;
mod symbol;
mod word;

use super::token::{TokenStream, Trivia};
use crate::lexer::{Cursor, LexError, LexemeIterator};
use std::{error::Error, fmt};

pub(super) use self::choice::{choice, choice_pair};
pub(super) use self::list::list;
pub(super) use self::optional::optional;
pub(super) use self::repeated::repeated;
pub(super) use self::sequence::{sequence, sequence_pair};
pub(super) use self::string::string;
pub(super) use self::symbol::symbol;
pub(super) use self::word::word;

/// A grammatical construct with a parser that builds its representation.
pub trait Syntax: Sized {
    fn parser() -> impl Parser<Output = Self>;
}

/// A parser that returns a value and the remaining input on success.
/// An unconsumed mismatch returns `None`; an error consumes the stream.
pub trait Parser: Sized {
    type Output;

    /// Describe the construct this parser recognizes.
    /// Used to report an unconsumed mismatch; errors after consumption describe
    /// the syntax needed at the failure position in `read` itself.
    fn description(&self) -> String;
    fn read<'s, I: LexemeIterator<'s>>(
        &self,
        tokens: TokenStream<'s, I>,
    ) -> Result<(Option<Self::Output>, TokenStream<'s, I>), ParseError>;

    /// Transform a matched result without changing the consumed input.
    fn map<O>(self, f: impl Fn(Self::Output) -> O) -> impl Parser<Output = O> {
        map::map(self, f)
    }

    /// Validate a matched result, reporting rejection at its starting position.
    fn try_map<O>(
        self,
        description: &'static str,
        f: impl Fn(Self::Output) -> Option<O>,
    ) -> impl Parser<Output = O> {
        map::try_map(self, description, f)
    }
}

/// Require EOF after trailing trivia, then collect the retained trivia.
pub(super) fn finish<'s>(
    mut tokens: TokenStream<'s, impl LexemeIterator<'s>>,
) -> Result<Vec<Trivia>, ParseError> {
    if let Some(token) = tokens.next_if(|_| true)? {
        return Err(ParseError::expected("end of input".into(), token.start));
    }
    Ok(tokens.into_trivia())
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

    /// Start of the unexpected token or input; EOF points just past the last character.
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
