//! Reading tokens from text or a UTF-8 stream.

mod cursor;
mod token;

pub use cursor::Cursor;
pub(super) use token::Token;

use crate::input::Input;
use std::{error::Error, fmt, io};

/// Failure to read the input or form a token.
#[derive(Debug)]
pub enum LexError {
    /// Reading or decoding a character failed at this position.
    Input { cursor: Cursor, cause: io::Error },
    /// The opening quote was found, but the source ended before a closing quote.
    UnterminatedString { opening: Cursor },
}

impl LexError {
    /// Start of the failed character read, or the unterminated string's opening quote.
    pub fn cursor(&self) -> &Cursor {
        match self {
            Self::Input { cursor, .. } => cursor,
            Self::UnterminatedString { opening } => opening,
        }
    }
}

impl fmt::Display for LexError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Input { cursor, cause } => write!(
                formatter,
                "{cause} at line {}, column {}",
                cursor.line, cursor.column
            ),
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
            Self::Input { cause, .. } => Some(cause),
            Self::UnterminatedString { .. } => None,
        }
    }
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

/// Scans tokens, borrowing string input or owning streamed text.
pub(super) struct Lexer<'input, 'source> {
    input: &'input mut dyn Input<'source>,
    lookahead: Option<Token<'source>>,
    state: Option<State>,
    cursor: Cursor,
    scanned: Cursor,
}

#[derive(Clone, Copy)]
enum State {
    Whitespace,
    AfterSlash { start: Cursor },
    LineComment,
    Word { start: Cursor },
    String { start: Cursor },
    StringEscape { start: Cursor },
}

impl<'input, 'source> Lexer<'input, 'source> {
    pub(super) fn new(input: &'input mut dyn Input<'source>) -> Self {
        Self {
            input,
            lookahead: None,
            cursor: Cursor::default(),
            scanned: Cursor::default(),
            state: Some(State::Whitespace),
        }
    }

    fn read_token(&mut self) -> Result<Option<Token<'source>>, LexError> {
        enum Step<'s> {
            Continue(State),
            OpenString,
            Finish(Token<'s>),
        }
        use Step::*;

        let token = loop {
            let Some(state) = self.state else {
                return Ok(None);
            };
            let cursor = self.scanned;
            let input_error = |cause| LexError::Input { cursor, cause };
            let ch = self.input.peek().map_err(input_error)?;
            let next = match (state, ch) {
                (State::Whitespace | State::LineComment, None) => {
                    self.state = None;
                    return Ok(None);
                }
                (State::Whitespace, Some(ch)) if ch.is_whitespace() => Continue(State::Whitespace),
                (State::Whitespace, Some('/')) => Continue(State::AfterSlash { start: cursor }),
                (State::Whitespace, Some('"')) => OpenString,
                (State::Whitespace, Some(ch)) => match Symbol::from_char(ch) {
                    Some(symbol) => Finish(Token::Symbol {
                        symbol,
                        start: cursor,
                    }),
                    None => {
                        self.input.begin_capture();
                        Continue(State::Word { start: cursor })
                    }
                },

                (State::AfterSlash { .. }, Some('/')) => Continue(State::LineComment),
                (State::AfterSlash { start }, _) => {
                    break Token::Symbol {
                        symbol: Symbol::Slash,
                        start,
                    };
                }
                (State::LineComment, Some('\r' | '\n')) => Continue(State::Whitespace),
                (State::LineComment, Some(_)) => Continue(State::LineComment),

                (State::Word { .. }, Some(ch))
                    if !ch.is_whitespace() && ch != '"' && Symbol::from_char(ch).is_none() =>
                {
                    Continue(state)
                }
                (State::Word { start }, _) => {
                    break Token::Word {
                        text: self.input.finish_capture(),
                        start,
                    };
                }

                (State::String { start }, Some('"')) => Finish(Token::String(token::Quoted {
                    content: self.input.finish_capture(),
                    start,
                })),
                (State::String { start }, Some('\\')) => Continue(State::StringEscape { start }),
                (State::String { .. }, Some(_)) => Continue(state),
                (State::StringEscape { start }, Some(_)) => Continue(State::String { start }),
                (State::String { start } | State::StringEscape { start }, None) => {
                    return Err(LexError::UnterminatedString { opening: start });
                }
            };
            self.input.advance().map_err(input_error)?;
            self.scanned =
                cursor.advanced([ch.expect("only a character transition advances the input")]);
            match next {
                Continue(state) => self.state = Some(state),
                OpenString => {
                    self.input.begin_capture();
                    self.state = Some(State::String { start: cursor });
                }
                Finish(kind) => break kind,
            }
        };
        self.state = Some(State::Whitespace);
        Ok(Some(token))
    }
}

impl<'source> Iterator for Lexer<'_, 'source> {
    type Item = Result<Token<'source>, LexError>;

    fn next(&mut self) -> Option<Self::Item> {
        let next = match self.lookahead.take() {
            Some(token) => Ok(Some(token)),
            None => self.read_token(),
        };
        if next.is_ok() {
            self.cursor = self.scanned;
        }
        next.transpose()
    }
}

impl<'s> Lexer<'_, 's> {
    /// Committed input boundary. Peeking never advances it.
    pub(super) fn cursor(&self) -> Cursor {
        self.cursor
    }

    pub(super) fn peek(&mut self) -> Result<Option<&Token<'s>>, LexError> {
        if self.lookahead.is_none() {
            self.lookahead = self.read_token()?;
        }
        Ok(self.lookahead.as_ref())
    }

    /// Start of the next token, or the end of the input after trailing trivia.
    pub(super) fn next_start(&mut self) -> Result<Cursor, LexError> {
        match self.peek()? {
            Some(token) => Ok(token.cursor()),
            None => Ok(self.scanned),
        }
    }

    /// Consume the token just matched by peek.
    pub(super) fn commit(&mut self) -> Token<'s> {
        let token = self.lookahead.take().expect("a matched token was peeked");
        self.cursor = self.scanned;
        token
    }
}

#[cfg(test)]
mod tests;
