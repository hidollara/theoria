use std::{
    borrow::Cow,
    io::{self, BufReader, ErrorKind, Read},
};

use super::{Cursor, Input, ReaderInput, StrInput};

// Run the shared Input contract against both implementations.
fn check_inputs<'source>(source: &'source str, check: impl Fn(&mut dyn Input<'source>)) {
    check(&mut StrInput::new(source));
    check(&mut ReaderInput::new(BufReader::with_capacity(
        1,
        source.as_bytes(),
    )));
}

fn position(cursor: &Cursor) -> (usize, usize, usize) {
    (cursor.offset, cursor.line, cursor.column)
}

#[test]
fn peek_preserves_position_and_captured_text() {
    check_inputs("幅x", |input| {
        assert_eq!(input.peek().unwrap(), Some('幅'));
        assert_eq!(input.peek().unwrap(), Some('幅'));
        assert_eq!(position(input.cursor()), (0, 1, 1));
        assert_eq!(input.take(), "");
    });
}

#[test]
fn next_consumes_the_peeked_character_exactly_once() {
    check_inputs("幅x", |input| {
        assert_eq!(input.peek().unwrap(), Some('幅'));
        assert_eq!(input.next().unwrap(), Some('幅'));
        assert_eq!(position(input.cursor()), (3, 1, 2));

        assert_eq!(input.peek().unwrap(), Some('x'));
        assert_eq!(input.take(), "幅");
        assert_eq!(input.next().unwrap(), Some('x'));
        assert_eq!(position(input.cursor()), (4, 1, 3));
        assert_eq!(input.take(), "x");
    });
}

#[test]
fn next_without_peek_counts_bytes_and_unicode_scalars_separately() {
    check_inputs("aé幅😀\u{301}\t\0", |input| {
        assert_eq!(position(input.cursor()), (0, 1, 1));
        for (character, offset, column) in [
            ('a', 1, 2),
            ('é', 3, 3),
            ('幅', 6, 4),
            ('😀', 10, 5),
            ('\u{301}', 12, 6), // A combining mark still counts as one scalar.
            ('\t', 13, 7),      // A tab does not advance to a display tab stop.
            ('\0', 14, 8),
        ] {
            assert_eq!(input.next().unwrap(), Some(character));
            assert_eq!(position(input.cursor()), (offset, 1, column));
        }
        assert_eq!(input.next().unwrap(), None);
    });
}

#[test]
fn lf_cr_and_crlf_each_end_one_line() {
    check_inputs("a\r\nb\rc\n\r\r\n", |input| {
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
            let before = *input.cursor();
            assert_eq!(input.peek().unwrap(), Some(character));
            assert_eq!(*input.cursor(), before);
            assert_eq!(input.next().unwrap(), Some(character));
            assert_eq!(position(input.cursor()), (offset, line, column));
        }
    });
}

#[test]
fn reset_discards_previous_capture_without_discarding_lookahead() {
    check_inputs("a幅", |input| {
        assert_eq!(input.next().unwrap(), Some('a'));
        assert_eq!(input.peek().unwrap(), Some('幅'));

        input.reset();
        assert_eq!(input.take(), "");
        assert_eq!(position(input.cursor()), (1, 1, 2));
        assert_eq!(input.next().unwrap(), Some('幅'));
        assert_eq!(input.take(), "幅");
    });
}

#[test]
fn take_resets_capture_between_characters_and_at_eof() {
    check_inputs("ab", |input| {
        assert_eq!(input.take(), "");
        assert_eq!(input.next().unwrap(), Some('a'));
        assert_eq!(input.take(), "a");
        assert_eq!(input.take(), "");
        assert_eq!(input.next().unwrap(), Some('b'));
        assert_eq!(input.next().unwrap(), None);
        assert_eq!(input.take(), "b");
        assert_eq!(input.take(), "");
    });
}

#[test]
fn eof_preserves_position_and_captured_text() {
    for source in ["", "幅\r\n"] {
        check_inputs(source, |input| {
            for character in source.chars() {
                assert_eq!(input.next().unwrap(), Some(character));
            }
            let end = *input.cursor();
            for _ in 0..3 {
                assert_eq!(input.peek().unwrap(), None);
                assert_eq!(input.next().unwrap(), None);
                assert_eq!(*input.cursor(), end);
            }
            assert_eq!(input.take(), source);
        });
    }
}

#[test]
fn string_capture_borrows_the_original_slice() {
    let source = String::from("a幅");
    let mut input = StrInput::new(&source);
    assert_eq!(input.next().unwrap(), Some('a'));
    input.reset();
    assert_eq!(input.next().unwrap(), Some('幅'));

    let Cow::Borrowed(text) = input.take() else {
        panic!("string input should borrow its captured text");
    };
    assert_eq!(text, "幅");
    assert_eq!(text.as_ptr(), source[1..].as_ptr());
}

#[test]
fn reader_capture_owns_text_independently_of_the_input() {
    let text: Cow<'static, str> = {
        let source = String::from("幅");
        let mut input = ReaderInput::new(source.as_bytes());
        assert_eq!(input.next().unwrap(), Some('幅'));
        input.take()
    };
    assert!(matches!(text, Cow::Owned(_)));
    assert_eq!(text, "幅");
}

#[test]
fn reader_decodes_unicode_boundaries_across_buffer_boundaries() {
    let characters = [
        '\0',
        '\u{7f}', // One-byte range.
        '\u{80}',
        '\u{7ff}', // Two-byte range.
        '\u{800}',
        '\u{d7ff}', // Three-byte range before surrogates.
        '\u{e000}',
        '\u{ffff}', // Three-byte range after surrogates.
        '\u{10000}',
        '\u{10ffff}', // Four-byte range.
    ];
    let source: String = characters.iter().collect();
    for capacity in 1..=source.len() {
        let mut input = ReaderInput::new(BufReader::with_capacity(capacity, source.as_bytes()));
        for character in characters {
            assert_eq!(
                input.next().unwrap(),
                Some(character),
                "capacity {capacity}"
            );
        }
        assert_eq!(input.next().unwrap(), None);
        assert_eq!(input.take(), source);
    }
}

#[test]
fn reader_rejects_invalid_utf8_without_consuming_a_logical_character() {
    for (reason, bytes) in [
        ("invalid leading byte", &[0xff][..]),
        ("isolated continuation", &[0x80]),
        ("overlong two-byte encoding", &[0xc0, 0xaf]),
        ("invalid four-byte leader", &[0xf5]),
        ("missing continuation", &[0xe5, b'A']),
        ("overlong three-byte encoding", &[0xe0, 0x80, 0x80]),
        ("surrogate", &[0xed, 0xa0, 0x80]),
        ("above Unicode maximum", &[0xf4, 0x90, 0x80, 0x80]),
    ] {
        let mut source = b"a\n".to_vec();
        source.extend_from_slice(bytes);
        let mut input = ReaderInput::new(BufReader::with_capacity(1, source.as_slice()));
        assert_eq!(input.next().unwrap(), Some('a'));
        assert_eq!(input.next().unwrap(), Some('\n'));

        let error = input.peek().unwrap_err();
        assert_eq!(error.cause.kind(), ErrorKind::InvalidData, "{reason}");
        assert_eq!(position(&error.start), (2, 2, 1), "{reason}");
        assert_eq!(*input.cursor(), error.start);
        assert_eq!(input.take(), "a\n");
    }
}

#[test]
fn reader_reports_truncated_utf8_as_invalid_data_instead_of_eof() {
    for character in ['é', '幅', '😀'] {
        let text = character.to_string();
        for length in 1..text.len() {
            let mut input = ReaderInput::new(&text.as_bytes()[..length]);
            let error = input.next().unwrap_err();
            assert_eq!(error.cause.kind(), ErrorKind::InvalidData);
            assert_eq!(position(&error.start), (0, 1, 1));
            assert_eq!(*input.cursor(), error.start);
            assert_eq!(input.take(), "");
        }
    }
}

#[test]
fn reader_does_not_read_past_the_peeked_character() {
    use ReadStep::Byte;
    // Exactly the bytes of 幅; any further read fails the test.
    let mut input = scripted_input(&[Byte(0xe5), Byte(0xb9), Byte(0x85)]);
    assert_eq!(input.peek().unwrap(), Some('幅'));
    assert_eq!(input.peek().unwrap(), Some('幅'));
    assert_eq!(input.next().unwrap(), Some('幅'));
}

#[test]
fn reader_does_not_read_again_after_eof() {
    let mut input = scripted_input(&[ReadStep::Eof]);
    assert_eq!(input.peek().unwrap(), None);
    assert_eq!(input.peek().unwrap(), None);
    assert_eq!(input.next().unwrap(), None);
}

#[test]
fn reader_retries_interruptions_before_and_within_a_character() {
    use ReadStep::{Byte, Eof, Error};
    let mut input = scripted_input(&[
        Error(ErrorKind::Interrupted),
        Byte(0xe5),
        Error(ErrorKind::Interrupted),
        Byte(0xb9),
        Error(ErrorKind::Interrupted),
        Byte(0x85),
        Error(ErrorKind::Interrupted),
        Eof,
    ]);
    assert_eq!(input.next().unwrap(), Some('幅'));
    assert_eq!(position(input.cursor()), (3, 1, 2));
    assert_eq!(input.next().unwrap(), None);
    assert_eq!(input.take(), "幅");
}

#[test]
fn reader_preserves_io_failures_before_and_within_a_character() {
    use ReadStep::{Byte, Error};
    for partial_character in [&[][..], &[Byte(0xe5), Byte(0xb9)]] {
        let mut steps = vec![Byte(b'a'), Byte(b'\n')];
        steps.extend_from_slice(partial_character);
        steps.push(Error(ErrorKind::PermissionDenied));
        let mut input = scripted_input(&steps);
        assert_eq!(input.next().unwrap(), Some('a'));
        assert_eq!(input.next().unwrap(), Some('\n'));

        let error = input.next().unwrap_err();
        assert_eq!(error.cause.kind(), ErrorKind::PermissionDenied);
        assert_eq!(error.cause.to_string(), "test read failure");
        assert_eq!(position(&error.start), (2, 2, 1));
        assert_eq!(*input.cursor(), error.start);
        assert_eq!(error.to_string(), "test read failure at line 2, column 1");
        assert_eq!(input.take(), "a\n");
        let cause = std::error::Error::source(&error).unwrap();
        assert_eq!(cause.to_string(), "test read failure");
    }
}

// Describe physical reads explicitly; reading past the script is a test failure.
#[derive(Clone, Copy)]
enum ReadStep {
    Byte(u8),
    Error(ErrorKind),
    Eof,
}

struct ScriptedReader<'a> {
    steps: &'a [ReadStep],
}

impl Read for ScriptedReader<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        let (step, rest) = self.steps.split_first().expect("unexpected extra read");
        self.steps = rest;
        match *step {
            ReadStep::Byte(byte) => {
                output[0] = byte;
                Ok(1)
            }
            ReadStep::Error(kind) => Err(io::Error::new(kind, "test read failure")),
            ReadStep::Eof => Ok(0),
        }
    }
}

fn scripted_input(steps: &[ReadStep]) -> ReaderInput<BufReader<ScriptedReader<'_>>> {
    ReaderInput::new(BufReader::with_capacity(1, ScriptedReader { steps }))
}
