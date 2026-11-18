//! Parsing lexemes into syntax, without name resolution or type checking.

pub(super) mod node;
pub(super) mod token;

use std::{error::Error, fmt, ops::Range};

use crate::lexer::{Cursor, LexError, LexemeIterator, Symbol};

pub use self::token::TokenStream;

/// A type that provides a parser for constructing itself.
pub trait Parsable: Sized {
    fn parser() -> impl Parser<Output = Self>;
}

/// A parser that returns a value and the remaining input on success.
/// An unconsumed mismatch returns `None`; an error consumes the stream.
pub trait Parser: Sized {
    type Output;

    fn expected(&self) -> Expected;

    fn read<'s, I: LexemeIterator<'s>>(
        &self,
        tokens: TokenStream<'s, I>,
    ) -> Result<(Option<Self::Output>, TokenStream<'s, I>), ParseError>;
}

/// Interpret a complete token stream using `T`'s grammar.
/// The AST remains independent of the collected trivia.
/// Both success and failure consume the stream.
pub fn parse<'s, T: Parsable>(
    tokens: TokenStream<'s, impl LexemeIterator<'s>>,
) -> Result<Parsed<T>, ParseError> {
    let parser = T::parser();
    let (syntax, mut tokens) = parser.read(tokens)?;
    let Some(syntax) = syntax else {
        return Err(tokens.error(parser.expected()));
    };
    let trivia = tokens.finish()?;
    Ok(Parsed { syntax, trivia })
}

/// Owned syntax and trivia, without retaining the original source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parsed<T> {
    syntax: T,
    /// Whitespace and comments in source order, without ownership by a syntax node.
    trivia: Vec<Trivia>,
}

impl<T> Parsed<T> {
    pub fn syntax(&self) -> &T {
        &self.syntax
    }
    pub fn trivia(&self) -> &[Trivia] {
        &self.trivia
    }
    /// Detach the AST from the source information for transformation or editing.
    pub fn into_syntax(self) -> T {
        self.syntax
    }
}

/// Whitespace or a comment retained independently of syntax nodes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trivia {
    pub kind: TriviaKind,
    /// Full UTF-8 byte range. Comment delimiters are included;
    /// a comment's line ending is separate whitespace trivia.
    pub span: Range<usize>,
    /// Start of the whitespace or comment, including the comment delimiter.
    pub cursor: Cursor,
}

/// Trivia text owned by the parse result, independent of the input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TriviaKind {
    Whitespace(Box<str>),
    /// The body after `//`, preserving spaces and excluding the line ending.
    LineComment(Box<str>),
}

/// What the parser needed at the error location.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expected {
    Symbol(Symbol),
    EndOfInput,
}

/// An input, lexical, or syntax error while parsing.
#[derive(Debug)]
pub enum ParseError {
    Lexical(LexError),
    Unexpected {
        /// UTF-8 byte range of the unexpected token, or an empty range at EOF.
        span: Range<usize>,
        cursor: Cursor,
        expected: Expected,
    },
}

impl ParseError {
    /// Start of the unexpected token or input; EOF points just past the last character.
    pub fn cursor(&self) -> &Cursor {
        match self {
            Self::Lexical(error) => error.cursor(),
            Self::Unexpected { cursor, .. } => cursor,
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (cursor, expected) = match self {
            Self::Lexical(error) => return fmt::Display::fmt(error, formatter),
            Self::Unexpected {
                cursor, expected, ..
            } => (cursor, expected),
        };
        formatter.write_str("expected ")?;
        match expected {
            Expected::Symbol(symbol) => write!(formatter, "'{symbol}'")?,
            Expected::EndOfInput => formatter.write_str("end of input")?,
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
