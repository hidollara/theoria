//! UTF-8 character input, text capture, and source positions.

use std::{
    borrow::Cow,
    error::Error,
    fmt,
    io::{self, BufRead},
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
    /// Return a copy advanced over one source character.
    pub fn advanced(mut self, ch: char) -> Self {
        self.advance(ch);
        self
    }

    pub(crate) fn advance(&mut self, ch: char) {
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

/// Reads UTF-8 characters and captures runs of matching text.
pub(super) trait Input<'source> {
    fn cursor(&self) -> &Cursor;
    fn peek(&mut self) -> Result<Option<char>, InputError>;
    fn eat(&mut self, expected: char) -> Result<bool, InputError>;
    /// Return the matching prefix, leaving the first mismatch unread.
    fn take_while(
        &mut self,
        matches: impl FnMut(char) -> bool,
    ) -> Result<Cow<'source, str>, InputError>;
}

pub(super) struct StrInput<'source> {
    source: &'source str,
    chars: Peekable<Chars<'source>>,
    cursor: Cursor,
}

impl<'source> StrInput<'source> {
    pub fn new(source: &'source str) -> Self {
        Self {
            source,
            chars: source.chars().peekable(),
            cursor: Cursor::default(),
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

    fn eat(&mut self, expected: char) -> Result<bool, InputError> {
        if self.chars.next_if_eq(&expected).is_some() {
            self.cursor.advance(expected);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn take_while(
        &mut self,
        mut matches: impl FnMut(char) -> bool,
    ) -> Result<Cow<'source, str>, InputError> {
        let start = self.cursor.offset;
        while let Some(ch) = self.chars.next_if(|ch| matches(*ch)) {
            self.cursor.advance(ch);
        }
        Ok(Cow::Borrowed(&self.source[start..self.cursor.offset]))
    }
}

pub(super) struct ReaderInput<R> {
    reader: R,
    // Retain a partial character if the reader returns an I/O error between bytes.
    buffered_bytes: [u8; 4],
    buffered_len: usize,
    cursor: Cursor,
    lookahead: Lookahead,
}

enum Lookahead {
    Unread,
    Character(char),
    Eoi,
}

impl<R: BufRead> ReaderInput<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            buffered_bytes: [0; 4],
            buffered_len: 0,
            cursor: Cursor::default(),
            lookahead: Lookahead::Unread,
        }
    }

    fn decode(&mut self) -> io::Result<Option<char>> {
        loop {
            let buffer = match self.reader.fill_buf() {
                Ok([]) if self.buffered_len == 0 => return Ok(None),
                Ok(buffer) => buffer,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(cause) => return Err(cause),
            };
            let count = buffer
                .len()
                .min(self.buffered_bytes.len() - self.buffered_len);
            let len = self.buffered_len + count;
            self.buffered_bytes[self.buffered_len..len].copy_from_slice(&buffer[..count]);
            let bytes = &self.buffered_bytes[..len];
            let text = match std::str::from_utf8(bytes) {
                Ok(text) => text,
                Err(error) if error.valid_up_to() > 0 => {
                    // The first character is complete even if a later one is malformed or partial.
                    std::str::from_utf8(&bytes[..error.valid_up_to()]).unwrap()
                }
                Err(error) if error.error_len().is_none() && count > 0 => {
                    self.reader.consume(count);
                    self.buffered_len = len;
                    continue;
                }
                Err(error) => {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, error));
                }
            };
            let ch = text.chars().next().unwrap();
            self.reader.consume(ch.len_utf8() - self.buffered_len);
            self.buffered_len = 0;
            return Ok(Some(ch));
        }
    }
}

impl<'source, R: BufRead> Input<'source> for ReaderInput<R> {
    fn cursor(&self) -> &Cursor {
        &self.cursor
    }

    fn peek(&mut self) -> Result<Option<char>, InputError> {
        match self.lookahead {
            Lookahead::Character(ch) => return Ok(Some(ch)),
            Lookahead::Eoi => return Ok(None),
            Lookahead::Unread => {}
        }
        let ch = self.decode().map_err(|cause| InputError {
            start: self.cursor,
            cause,
        })?;
        self.lookahead = match ch {
            Some(ch) => Lookahead::Character(ch),
            None => Lookahead::Eoi,
        };
        Ok(ch)
    }

    fn eat(&mut self, expected: char) -> Result<bool, InputError> {
        if self.peek()? == Some(expected) {
            self.lookahead = Lookahead::Unread;
            self.cursor.advance(expected);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn take_while(
        &mut self,
        mut matches: impl FnMut(char) -> bool,
    ) -> Result<Cow<'source, str>, InputError> {
        let mut text = String::new();
        while let Some(ch) = self.peek()?.filter(|ch| matches(*ch)) {
            self.lookahead = Lookahead::Unread;
            self.cursor.advance(ch);
            text.push(ch);
        }
        Ok(Cow::Owned(text))
    }
}

#[cfg(test)]
mod tests;
