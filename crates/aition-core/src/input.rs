//! UTF-8 character input and text capture.

use std::{
    borrow::Cow,
    io::{self, BufRead},
    str::{self, Chars},
};

pub use crate::ito::Cursor;

/// Reads UTF-8 characters and captures runs of matching text.
pub(super) trait Input<'source> {
    /// Inspect the next character without consuming it or accumulating text.
    fn peek(&mut self) -> io::Result<Option<char>>;
    /// Consume one character, if present.
    /// After a successful peek, this uses the peeked character without another read.
    fn advance(&mut self) -> io::Result<()>;
    /// Start a fresh text capture at the current position.
    fn begin_capture(&mut self);
    /// Finish the capture and return the characters consumed since it started.
    /// With no active capture, return empty text.
    fn finish_capture(&mut self) -> Cow<'source, str>;
}

pub(super) struct StrInput<'source> {
    text_start: Option<&'source str>,
    chars: Chars<'source>,
}

impl<'source> StrInput<'source> {
    pub fn new(source: &'source str) -> Self {
        Self {
            text_start: None,
            chars: source.chars(),
        }
    }
}

impl<'source> Input<'source> for StrInput<'source> {
    fn peek(&mut self) -> io::Result<Option<char>> {
        Ok(self.chars.clone().next())
    }

    fn advance(&mut self) -> io::Result<()> {
        self.chars.next();
        Ok(())
    }

    fn begin_capture(&mut self) {
        self.text_start = Some(self.chars.as_str());
    }

    fn finish_capture(&mut self) -> Cow<'source, str> {
        let Some(start) = self.text_start.take() else {
            return Cow::Borrowed("");
        };
        Cow::Borrowed(&start[..start.len() - self.chars.as_str().len()])
    }
}

pub(super) struct ReaderInput<R> {
    reader: R,
    text: Option<String>,
    lookahead: Option<Lookahead>,
}

enum Lookahead {
    Partial { bytes: [u8; 4], len: usize },
    Ready(Option<char>),
}

impl<R: BufRead> ReaderInput<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            text: None,
            lookahead: None,
        }
    }
}

impl<'source, R: BufRead> Input<'source> for ReaderInput<R> {
    fn peek(&mut self) -> io::Result<Option<char>> {
        let (mut bytes, mut len) = match self.lookahead {
            Some(Lookahead::Ready(next)) => return Ok(next),
            Some(Lookahead::Partial { bytes, len }) => (bytes, len),
            None => ([0; 4], 0),
        };
        let next = loop {
            let buffer = match self.reader.fill_buf() {
                Ok(buffer) => buffer,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(cause) => return Err(cause),
            };
            let Some(&first) = bytes[..len].first().or_else(|| buffer.first()) else {
                break None;
            };
            let width = match first {
                0xc2..=0xdf => 2, // 110xxxxx (C0 and C1 would encode ASCII overlong).
                0xe0..=0xef => 3, // 1110xxxx
                0xf0..=0xf4 => 4, // 11110xxx (F5 through F7 would exceed U+10FFFF).
                _ => 1,           // 0xxxxxxx (ASCII) or an invalid leading byte.
            };
            let count = buffer.len().min(width - len);
            bytes[len..len + count].copy_from_slice(&buffer[..count]);
            len += count;
            match str::from_utf8(&bytes[..len]) {
                Ok(text) => {
                    self.reader.consume(count);
                    break text.chars().next();
                }
                Err(error) if error.error_len().is_none() && count > 0 => {
                    self.reader.consume(count);
                    self.lookahead = Some(Lookahead::Partial { bytes, len });
                }
                Err(error) => return Err(io::Error::new(io::ErrorKind::InvalidData, error)),
            }
        };
        self.lookahead = Some(Lookahead::Ready(next));
        Ok(next)
    }

    fn advance(&mut self) -> io::Result<()> {
        if let Some(ch) = self.peek()? {
            self.lookahead = None;
            if let Some(text) = &mut self.text {
                text.push(ch);
            }
        }
        Ok(())
    }

    fn begin_capture(&mut self) {
        self.text = Some(String::new());
    }

    fn finish_capture(&mut self) -> Cow<'source, str> {
        self.text.take().map_or(Cow::Borrowed(""), Cow::Owned)
    }
}

#[cfg(test)]
mod tests;
