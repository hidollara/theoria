//! Reading lexemes from text or a UTF-8 stream, without interpreting syntax.
//!
//! ```
//! use aition_core::lexer::{self, LexemeKind};
//!
//! let lexeme = lexer::from_str("Panel").next().unwrap().unwrap();
//! assert_eq!(lexeme.kind, LexemeKind::Word("Panel".into()));
//! ```
//!
//! Buffered input can be lexed independently of the parser as well:
//!
//! ```
//! use std::io::BufReader;
//! use aition_core::lexer::{self, LexemeKind};
//!
//! let reader = BufReader::new("Panel".as_bytes());
//! let lexeme = lexer::from_reader(reader).next().unwrap().unwrap();
//! assert_eq!(lexeme.kind, LexemeKind::Word("Panel".into()));
//! ```

mod input;

use self::input::{Input, ReaderInput, StrInput};
use std::{borrow::Cow, error::Error, fmt, io::BufRead, marker::PhantomData, ops::Range};

pub use self::input::{Cursor, InputError};

/// Scan borrowed text one lexeme at a time, stopping after EOF or the first error.
pub fn from_str(source: &str) -> impl LexemeIterator<'_> {
    Lexer::new(StrInput::new(source))
}

/// Scan buffered UTF-8 input, returning lexemes that own their text.
/// The iterator retains the reader; yielded lexemes do not borrow it.
/// Stops after EOF or the first error.
pub fn from_reader(reader: impl BufRead) -> impl LexemeIterator<'static> {
    Lexer::new(ReaderInput::new(reader))
}

/// Failure to read the input or form a lexeme.
#[derive(Debug)]
pub enum LexError {
    Input(InputError),
    /// The opening quote was found, but the source ended before a closing quote.
    UnterminatedString {
        opening: Cursor,
    },
}

impl LexError {
    /// Start of the failed character read, or the unterminated string's opening quote.
    pub fn cursor(&self) -> &Cursor {
        match self {
            Self::Input(error) => &error.start,
            Self::UnterminatedString { opening } => opening,
        }
    }
}

impl fmt::Display for LexError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Input(error) => fmt::Display::fmt(error, formatter),
            Self::UnterminatedString { opening } => write!(
                formatter,
                "unterminated string at line {}, column {}",
                opening.line, opening.column,
            ),
        }
    }
}

impl Error for LexError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Input(error) => Some(error),
            Self::UnterminatedString { .. } => None,
        }
    }
}

impl From<InputError> for LexError {
    fn from(error: InputError) -> Self {
        Self::Input(error)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Lexeme<'source> {
    pub kind: LexemeKind<'source>,
    /// Start of the lexeme, including its opening delimiter.
    pub start: Cursor,
    /// Position just after the lexeme, including its closing delimiter.
    /// A comment's line ending is separate whitespace.
    pub end: Cursor,
}

impl Lexeme<'_> {
    /// Full UTF-8 byte range, including quotes or comment delimiters.
    pub fn span(&self) -> Range<usize> {
        self.start.offset..self.end.offset
    }
}

/// An iterator yielding lexemes or lexical errors.
/// Implemented automatically for every iterator with this item type.
pub trait LexemeIterator<'source>: Iterator<Item = Result<Lexeme<'source>, LexError>> {}

impl<'source, I> LexemeIterator<'source> for I where
    I: Iterator<Item = Result<Lexeme<'source>, LexError>> + ?Sized
{
}

#[derive(Debug, PartialEq, Eq)]
pub enum LexemeKind<'source> {
    /// An unclassified word; the parser validates its spelling for its role.
    Word(Cow<'source, str>),
    /// Contents between quotes, without escape decoding.
    String(Cow<'source, str>),
    LineComment(Cow<'source, str>),
    Whitespace(Cow<'source, str>),
    Symbol(Symbol),
}

/// Punctuation recognized by the lexer, independent of its grammatical role.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Symbol {
    OpenParen,
    CloseParen,
    OpenBracket,
    CloseBracket,
    OpenBrace,
    CloseBrace,
    OpenAngle,
    CloseAngle,
    Dot,
    Comma,
    Semicolon,
    Colon,
    Equal,
    At,
    Pipe,
    Ampersand,
    Question,
    Asterisk,
    Slash,
}

impl Symbol {
    pub fn from_char(ch: char) -> Option<Self> {
        Some(match ch {
            '(' => Self::OpenParen,
            ')' => Self::CloseParen,
            '[' => Self::OpenBracket,
            ']' => Self::CloseBracket,
            '{' => Self::OpenBrace,
            '}' => Self::CloseBrace,
            '<' => Self::OpenAngle,
            '>' => Self::CloseAngle,
            '.' => Self::Dot,
            ',' => Self::Comma,
            ';' => Self::Semicolon,
            ':' => Self::Colon,
            '=' => Self::Equal,
            '@' => Self::At,
            '|' => Self::Pipe,
            '&' => Self::Ampersand,
            '?' => Self::Question,
            '*' => Self::Asterisk,
            '/' => Self::Slash,
            _ => return None,
        })
    }

    pub fn as_char(self) -> char {
        match self {
            Self::OpenParen => '(',
            Self::CloseParen => ')',
            Self::OpenBracket => '[',
            Self::CloseBracket => ']',
            Self::OpenBrace => '{',
            Self::CloseBrace => '}',
            Self::OpenAngle => '<',
            Self::CloseAngle => '>',
            Self::Dot => '.',
            Self::Comma => ',',
            Self::Semicolon => ';',
            Self::Colon => ':',
            Self::Equal => '=',
            Self::At => '@',
            Self::Pipe => '|',
            Self::Ampersand => '&',
            Self::Question => '?',
            Self::Asterisk => '*',
            Self::Slash => '/',
        }
    }
}

impl fmt::Display for Symbol {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_char().fmt(formatter)
    }
}

/// Scans one lexeme at a time, borrowing string input or owning streamed text.
/// Stops after the first error.
struct Lexer<'source, I> {
    input: I,
    ended: bool,
    source: PhantomData<&'source str>,
}

impl<'source, I: Input<'source>> Lexer<'source, I> {
    fn new(input: I) -> Self {
        Self {
            input,
            ended: false,
            source: PhantomData,
        }
    }

    fn lexeme(&mut self) -> Result<Option<Lexeme<'source>>, LexError> {
        self.input.reset();
        let start = *self.input.cursor();
        let Some(first) = self.input.next()? else {
            return Ok(None);
        };
        let kind = match first {
            '"' => {
                self.input.reset();
                loop {
                    match self.input.peek()? {
                        Some('"') => {
                            let text = self.input.take();
                            self.input.next()?;
                            break LexemeKind::String(text);
                        }
                        Some('\\') => {
                            // An escaped quote must not end the string.
                            self.input.next()?;
                            self.input.next()?;
                        }
                        Some(_) => {
                            self.input.next()?;
                        }
                        None => {
                            return Err(LexError::UnterminatedString { opening: start });
                        }
                    }
                }
            }
            '/' if self.input.peek()? == Some('/') => {
                self.input.next()?;
                self.input.reset();
                while self
                    .input
                    .peek()?
                    .is_some_and(|ch| ch != '\r' && ch != '\n')
                {
                    self.input.next()?;
                }
                LexemeKind::LineComment(self.input.take())
            }
            ch if ch.is_whitespace() => {
                while self.input.peek()?.is_some_and(char::is_whitespace) {
                    self.input.next()?;
                }
                LexemeKind::Whitespace(self.input.take())
            }
            ch => match Symbol::from_char(ch) {
                Some(symbol) => LexemeKind::Symbol(symbol),
                None => {
                    while self.input.peek()?.is_some_and(|ch| {
                        !ch.is_whitespace() && ch != '"' && Symbol::from_char(ch).is_none()
                    }) {
                        self.input.next()?;
                    }
                    LexemeKind::Word(self.input.take())
                }
            },
        };
        Ok(Some(Lexeme {
            kind,
            start,
            end: *self.input.cursor(),
        }))
    }
}

impl<'source, I: Input<'source>> Iterator for Lexer<'source, I> {
    type Item = Result<Lexeme<'source>, LexError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.ended {
            return None;
        }
        match self.lexeme() {
            Ok(Some(lexeme)) => Some(Ok(lexeme)),
            Ok(None) => {
                self.ended = true;
                None
            }
            Err(error) => {
                self.ended = true;
                Some(Err(error))
            }
        }
    }
}

#[cfg(test)]
mod tests;
