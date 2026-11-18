//! Reading tokens from text or a UTF-8 stream, without interpreting syntax.

use crate::input::{Cursor, Input, InputError, ReaderInput, StrInput};
use std::{borrow::Cow, error::Error, fmt, io::BufRead};

/// Scan borrowed text into tokens.
pub fn from_str(source: &str) -> impl TokenStream<'_> {
    Lexer {
        input: StrInput::new(source),
    }
}

/// Scan buffered UTF-8 input, returning tokens that own their text.
/// The stream owns the reader and returns tokens independent of its lifetime.
pub fn from_reader(reader: impl BufRead) -> impl TokenStream<'static> {
    Lexer {
        input: ReaderInput::new(reader),
    }
}

/// Failure to read the input or form a token.
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

/// Every read after the end of input returns the same final cursor.
/// A UTF-8 decoding error leaves the stream at the malformed character.
pub trait TokenStream<'source> {
    fn next(&mut self) -> Result<TokenStreamItem<'source>, LexError>;
}

/// The next token or the position where input ends.
#[derive(Debug, PartialEq, Eq)]
pub enum TokenStreamItem<'source> {
    Token(Token<'source>),
    Eoi(Cursor),
}

#[derive(Debug, PartialEq, Eq)]
pub struct Token<'source> {
    pub kind: TokenKind<'source>,
    /// Start of the token, including its opening delimiter.
    pub start: Cursor,
    /// Position just after the token, including its closing delimiter.
    /// A comment's line ending is separate whitespace.
    pub end: Cursor,
}

impl Token<'_> {
    /// Start of the retained text, after any delimiter removed by the lexer.
    pub fn content_start(&self) -> Cursor {
        match &self.kind {
            TokenKind::String(_) => self.start.advanced('"'),
            TokenKind::LineComment(_) => self.start.advanced('/').advanced('/'),
            _ => self.start,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum TokenKind<'source> {
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

/// Scans one token at a time, borrowing string input or owning streamed text.
struct Lexer<I> {
    input: I,
}

impl<'source, I: Input<'source>> TokenStream<'source> for Lexer<I> {
    fn next(&mut self) -> Result<TokenStreamItem<'source>, LexError> {
        let start = *self.input.cursor();
        let kind = match self.input.peek()? {
            None => return Ok(TokenStreamItem::Eoi(start)),
            Some('"') => {
                self.input.eat('"')?;
                let text = self.input.take_while({
                    let mut escaped = false;
                    move |ch| {
                        if escaped {
                            escaped = false;
                            return true;
                        }
                        escaped = ch == '\\';
                        ch != '"'
                    }
                })?;
                if !self.input.eat('"')? {
                    return Err(LexError::UnterminatedString { opening: start });
                }
                TokenKind::String(text)
            }
            Some(ch) if ch.is_whitespace() => {
                TokenKind::Whitespace(self.input.take_while(char::is_whitespace)?)
            }
            Some(ch) => match Symbol::from_char(ch) {
                Some(symbol) => {
                    self.input.eat(ch)?;
                    if symbol == Symbol::Slash && self.input.eat('/')? {
                        TokenKind::LineComment(
                            self.input.take_while(|ch| ch != '\r' && ch != '\n')?,
                        )
                    } else {
                        TokenKind::Symbol(symbol)
                    }
                }
                None => TokenKind::Word(self.input.take_while(|ch| {
                    !ch.is_whitespace() && ch != '"' && Symbol::from_char(ch).is_none()
                })?),
            },
        };
        Ok(TokenStreamItem::Token(Token {
            kind,
            start,
            end: *self.input.cursor(),
        }))
    }
}

#[cfg(test)]
mod tests;
