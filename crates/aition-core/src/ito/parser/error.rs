use crate::ito::Cursor;
use crate::ito::lexer::LexError;
use std::{error::Error, fmt};

/// An input, lexical, or syntax error while parsing.
#[derive(Debug)]
pub enum ParseError {
    Lexical(LexError),
    Unexpected {
        /// A description of the expected syntax.
        expected: String,
        cursor: Cursor,
    },
}

impl ParseError {
    /// Report a grammar expectation at the position selected by the parser.
    pub(super) fn expected(expected: impl fmt::Display, cursor: Cursor) -> Self {
        Self::Unexpected {
            expected: expected.to_string(),
            cursor,
        }
    }

    pub(super) fn expecting(mut self, description: impl fmt::Display) -> Self {
        if let Self::Unexpected { expected, .. } = &mut self {
            *expected = description.to_string();
        }
        self
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
            Self::Unexpected { expected, cursor } => (expected, cursor),
        };
        if expected.is_empty() {
            formatter.write_str("unexpected input")?;
        } else {
            write!(formatter, "expected {expected}")?;
        }
        write!(
            formatter,
            " at line {}, column {}",
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
