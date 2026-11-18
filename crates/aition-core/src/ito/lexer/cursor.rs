/// The start of a UTF-8 character, or the boundary just past the final character.
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
    /// Return a copy advanced over the given source characters.
    pub fn advanced(mut self, chars: impl IntoIterator<Item = char>) -> Self {
        for ch in chars {
            self.advance(ch);
        }
        self
    }

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

#[cfg(test)]
mod tests {
    use super::Cursor;

    fn position(cursor: &Cursor) -> (usize, usize, usize) {
        (cursor.offset, cursor.line, cursor.column)
    }

    #[test]
    fn cursor_counts_bytes_and_unicode_scalars_separately() {
        let mut cursor = Cursor::default();
        for (character, offset, column) in [
            ('a', 1, 2),
            ('é', 3, 3),
            ('幅', 6, 4),
            ('😀', 10, 5),
            ('\u{301}', 12, 6), // A combining mark still counts as one scalar.
            ('\t', 13, 7),      // A tab does not advance to a display tab stop.
            ('\0', 14, 8),
        ] {
            cursor = cursor.advanced([character]);
            assert_eq!(position(&cursor), (offset, 1, column));
        }
    }

    #[test]
    fn lf_cr_and_crlf_each_end_one_line() {
        let mut cursor = Cursor::default();
        for (character, offset, line, column) in [
            ('a', 1, 1, 2),
            ('\r', 2, 2, 1),
            ('\n', 3, 2, 1), // CRLF advances the line at CR, not again at LF.
            ('b', 4, 2, 2),
            ('\r', 5, 3, 1),
            ('c', 6, 3, 2),
            ('\n', 7, 4, 1),
            ('\r', 8, 5, 1),
            ('\r', 9, 6, 1),
            ('\n', 10, 6, 1),
        ] {
            cursor = cursor.advanced([character]);
            assert_eq!(position(&cursor), (offset, line, column));
        }
    }
}
