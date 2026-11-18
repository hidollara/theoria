//! Character input, text capture, and source positions used by the lexer.

use std::{
    borrow::Cow,
    error::Error,
    fmt,
    io::{self, BufRead, Bytes},
    iter::Peekable,
    str::Chars,
};

/// A position in the input, also used as a snapshot on tokens and diagnostics.
/// Offset counts UTF-8 bytes from zero; line and column start at one.
/// Columns count Unicode scalar values, including tabs and combining characters.
/// LF, CRLF, and CR each end a line; display width is not measured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cursor {
    pub offset: usize,
    pub line: usize,
    pub column: usize,
    after_cr: bool,
}

impl Default for Cursor {
    fn default() -> Self {
        Self {
            offset: 0,
            line: 1,
            column: 1,
            after_cr: false,
        }
    }
}

impl Cursor {
    fn advance(&mut self, ch: char) {
        self.offset += ch.len_utf8();
        match ch {
            '\r' => {
                self.line += 1;
                self.column = 1;
            }
            '\n' if self.after_cr => {}
            '\n' => {
                self.line += 1;
                self.column = 1;
            }
            _ => self.column += 1,
        }
        self.after_cr = ch == '\r';
    }
}

/// A read failure or invalid UTF-8 sequence, located in the original byte stream.
#[derive(Debug)]
pub struct InputError {
    /// Position where reading the character began, even if some bytes were read.
    pub start: Cursor,
    pub cause: io::Error,
}

impl fmt::Display for InputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} at line {}, column {}",
            self.cause, self.start.line, self.start.column
        )
    }
}

impl Error for InputError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.cause)
    }
}

/// Supplies characters and the text consumed since the last take or reset.
/// Text is captured from the beginning of the input without an initial reset.
/// Peeking does not advance the logical position or enter the captured text.
pub(super) trait Input<'source> {
    fn cursor(&self) -> &Cursor;
    fn peek(&mut self) -> Result<Option<char>, InputError>;
    fn next(&mut self) -> Result<Option<char>, InputError>;
    /// Discard captured text without advancing the cursor or clearing lookahead.
    fn reset(&mut self);
    /// Return captured text and reset capture at the current cursor.
    /// The cursor and any unconsumed lookahead are unchanged.
    fn take(&mut self) -> Cow<'source, str>;
}

pub(super) struct StrInput<'source> {
    source: &'source str,
    chars: Peekable<Chars<'source>>,
    cursor: Cursor,
    text_start: usize,
}

impl<'source> StrInput<'source> {
    pub fn new(source: &'source str) -> Self {
        Self {
            source,
            chars: source.chars().peekable(),
            cursor: Cursor::default(),
            text_start: 0,
        }
    }
}

impl<'source> Input<'source> for StrInput<'source> {
    fn cursor(&self) -> &Cursor {
        &self.cursor
    }

    fn peek(&mut self) -> Result<Option<char>, InputError> {
        Ok(self.chars.peek().copied())
    }

    fn next(&mut self) -> Result<Option<char>, InputError> {
        let ch = self.chars.next();
        if let Some(ch) = ch {
            self.cursor.advance(ch);
        }
        Ok(ch)
    }

    fn reset(&mut self) {
        self.text_start = self.cursor.offset;
    }

    fn take(&mut self) -> Cow<'source, str> {
        let text = &self.source[self.text_start..self.cursor.offset];
        self.text_start = self.cursor.offset;
        Cow::Borrowed(text)
    }
}

pub(super) struct ReaderInput<R> {
    bytes: Bytes<R>,
    cursor: Cursor,
    lookahead: Lookahead,
    text: String,
}

enum Lookahead {
    Unread,
    Character(char),
    Eoi,
}

impl<R: BufRead> ReaderInput<R> {
    pub fn new(reader: R) -> Self {
        Self {
            bytes: reader.bytes(),
            cursor: Cursor::default(),
            lookahead: Lookahead::Unread,
            text: String::new(),
        }
    }

    fn decode(&mut self) -> Result<Option<char>, InputError> {
        // Validate each prefix with the standard decoder. A character may cross
        // any number of reader buffers, but needs at most four UTF-8 bytes.
        let mut bytes = [0; 4];
        for length in 0..bytes.len() {
            let byte = self.bytes.next().transpose().map_err(|cause| InputError {
                start: self.cursor,
                cause,
            })?;
            let Some(byte) = byte else {
                if length == 0 {
                    return Ok(None);
                }
                return Err(InputError {
                    start: self.cursor,
                    cause: io::Error::new(io::ErrorKind::InvalidData, "incomplete UTF-8 character"),
                });
            };
            bytes[length] = byte;
            match std::str::from_utf8(&bytes[..=length]) {
                Ok(text) => return Ok(text.chars().next()),
                Err(error) if error.error_len().is_none() => {}
                Err(error) => {
                    return Err(InputError {
                        start: self.cursor,
                        cause: io::Error::new(io::ErrorKind::InvalidData, error),
                    });
                }
            }
        }
        unreachable!("a UTF-8 character cannot exceed four bytes")
    }
}

impl<'source, R: BufRead> Input<'source> for ReaderInput<R> {
    fn cursor(&self) -> &Cursor {
        &self.cursor
    }

    fn peek(&mut self) -> Result<Option<char>, InputError> {
        Ok(match self.lookahead {
            Lookahead::Unread => {
                let ch = self.decode()?;
                self.lookahead = match ch {
                    Some(ch) => Lookahead::Character(ch),
                    None => Lookahead::Eoi,
                };
                ch
            }
            Lookahead::Character(ch) => Some(ch),
            Lookahead::Eoi => None,
        })
    }

    fn next(&mut self) -> Result<Option<char>, InputError> {
        let ch = self.peek()?;
        if let Some(ch) = ch {
            self.lookahead = Lookahead::Unread;
            self.cursor.advance(ch);
            self.text.push(ch);
        }
        Ok(ch)
    }

    fn reset(&mut self) {
        self.text.clear();
    }

    fn take(&mut self) -> Cow<'source, str> {
        Cow::Owned(std::mem::take(&mut self.text))
    }
}

#[cfg(test)]
mod tests;
