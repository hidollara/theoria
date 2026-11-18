use super::{Cursor, Input, ReaderInput, StrInput};
use std::{
    borrow::Cow,
    io::{self, BufReader, ErrorKind, Read},
};

// Run the shared Input contract against both implementations.
macro_rules! check_inputs {
    ($source:expr, |$input:ident| $check:block) => {{
        let source = $source;
        {
            let $input = &mut StrInput::new(source);
            $check
        }
        {
            let $input = &mut ReaderInput::new(BufReader::with_capacity(1, source.as_bytes()));
            $check
        }
    }};
}

fn position(cursor: &Cursor) -> (usize, usize, usize) {
    (cursor.offset, cursor.line, cursor.column)
}

#[test]
fn peek_and_mismatches_leave_the_next_character_unread() {
    check_inputs!("幅x", |input| {
        assert_eq!(input.peek().unwrap(), Some('幅'));
        assert!(!input.eat('x').unwrap());
        assert_eq!(input.take_while(|ch| ch == 'x').unwrap(), "");
        assert_eq!(position(input.cursor()), (0, 1, 1));
        assert!(input.eat('幅').unwrap());
        assert_eq!(input.peek().unwrap(), Some('x'));
    });
}

#[test]
fn take_while_captures_only_the_requested_run() {
    check_inputs!("幅x,é", |input| {
        assert_eq!(input.take_while(char::is_alphabetic).unwrap(), "幅x");
        assert_eq!(position(input.cursor()), (4, 1, 3));
        assert!(input.eat(',').unwrap());
        assert_eq!(input.take_while(char::is_alphabetic).unwrap(), "é");
        assert_eq!(input.take_while(|_| true).unwrap(), "");
    });
}

#[test]
fn consuming_a_character_counts_bytes_and_unicode_scalars_separately() {
    check_inputs!("aé幅😀\u{301}\t\0", |input| {
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
            assert!(input.eat(character).unwrap());
            assert_eq!(position(input.cursor()), (offset, 1, column));
        }
        assert_eq!(input.peek().unwrap(), None);
    });
}

#[test]
fn lf_cr_and_crlf_each_end_one_line() {
    check_inputs!("a\r\nb\rc\n\r\r\n", |input| {
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
            assert!(input.eat(character).unwrap());
            assert_eq!(position(input.cursor()), (offset, line, column));
        }
    });
}

#[test]
fn eoi_preserves_position() {
    for source in ["", "幅\r\n"] {
        check_inputs!(source, |input| {
            assert_eq!(input.take_while(|_| true).unwrap(), source);
            let end = *input.cursor();
            for _ in 0..3 {
                assert_eq!(input.peek().unwrap(), None);
                assert_eq!(*input.cursor(), end);
            }
            assert!(!input.eat('x').unwrap());
        });
    }
}

#[test]
fn string_capture_borrows_the_original_slice() {
    let source = String::from("a幅");
    let mut input = StrInput::new(&source);
    assert!(input.eat('a').unwrap());
    let Cow::Borrowed(text) = input.take_while(|_| true).unwrap() else {
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
        input.take_while(|_| true).unwrap()
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
        assert_eq!(
            input.take_while(|_| true).unwrap(),
            source,
            "capacity {capacity}"
        );
        assert_eq!(input.peek().unwrap(), None);
    }
}

#[test]
fn reader_consumes_one_character_when_its_buffer_also_contains_invalid_utf8() {
    let bytes = [b'a', 0xc3, 0xa9, 0xff];
    for capacity in 1..=bytes.len() {
        let mut input = ReaderInput::new(BufReader::with_capacity(capacity, bytes.as_slice()));
        assert!(input.eat('a').unwrap());
        assert!(input.eat('é').unwrap());
        let error = input.take_while(|_| true).unwrap_err();
        assert_eq!(error.cause.kind(), ErrorKind::InvalidData);
        assert_eq!(position(&error.start), (3, 1, 3));
    }
}

#[test]
fn reader_rejects_invalid_utf8_at_the_start_of_the_malformed_sequence() {
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
        assert!(input.eat('a').unwrap());
        assert!(input.eat('\n').unwrap());

        let peek_error = input.peek().unwrap_err();
        assert_eq!(peek_error.cause.kind(), ErrorKind::InvalidData);
        assert_eq!(position(&peek_error.start), (2, 2, 1));
        assert_eq!(*input.cursor(), peek_error.start);
        let error = input.take_while(|_| true).unwrap_err();
        assert_eq!(error.cause.kind(), ErrorKind::InvalidData, "{reason}");
        let cause = error
            .cause
            .get_ref()
            .unwrap()
            .downcast_ref::<std::str::Utf8Error>();
        assert_eq!(
            cause,
            Some(&std::str::from_utf8(bytes).unwrap_err()),
            "{reason}"
        );
        assert_eq!(position(&error.start), (2, 2, 1), "{reason}");
        assert_eq!(*input.cursor(), error.start, "{reason}");
    }
}

#[test]
fn reader_reports_truncated_utf8_as_invalid_data_instead_of_eoi() {
    for character in ['é', '幅', '😀'] {
        let text = character.to_string();
        for length in 1..text.len() {
            let bytes = &text.as_bytes()[..length];
            let mut input = ReaderInput::new(bytes);
            let error = input.take_while(|_| true).unwrap_err();
            assert_eq!(error.cause.kind(), ErrorKind::InvalidData);
            let cause = error
                .cause
                .get_ref()
                .unwrap()
                .downcast_ref::<std::str::Utf8Error>();
            assert_eq!(cause, Some(&std::str::from_utf8(bytes).unwrap_err()));
            assert_eq!(position(&error.start), (0, 1, 1));
            assert_eq!(*input.cursor(), error.start);

            assert_eq!(input.take_while(|_| true).unwrap_err().start, error.start);
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
    assert!(input.eat('幅').unwrap());
}

#[test]
fn reader_does_not_read_again_after_eoi() {
    let mut input = scripted_input(&[ReadStep::Eoi]);
    for _ in 0..3 {
        assert_eq!(input.peek().unwrap(), None);
    }
}

#[test]
fn reader_retries_interruptions_before_and_within_a_character() {
    use ReadStep::{Byte, Eoi, Error};
    let mut input = scripted_input(&[
        Error(ErrorKind::Interrupted),
        Byte(0xe5),
        Error(ErrorKind::Interrupted),
        Byte(0xb9),
        Error(ErrorKind::Interrupted),
        Byte(0x85),
        Error(ErrorKind::Interrupted),
        Eoi,
    ]);
    assert!(input.eat('幅').unwrap());
    assert_eq!(position(input.cursor()), (3, 1, 2));
    assert_eq!(input.peek().unwrap(), None);
}

#[test]
fn reader_preserves_io_failures_before_and_within_a_character() {
    use ReadStep::{Byte, Error};
    for partial_character in [&[][..], &[Byte(0xe5), Byte(0xb9)]] {
        let mut steps = vec![Byte(b'a'), Byte(b'\n')];
        steps.extend_from_slice(partial_character);
        steps.push(Error(ErrorKind::PermissionDenied));
        let mut input = scripted_input(&steps);
        assert!(input.eat('a').unwrap());
        assert!(input.eat('\n').unwrap());

        let error = input.take_while(|_| true).unwrap_err();
        assert_eq!(error.cause.kind(), ErrorKind::PermissionDenied);
        assert_eq!(error.cause.to_string(), "test read failure");
        assert_eq!(position(&error.start), (2, 2, 1));
        assert_eq!(*input.cursor(), error.start);
        assert_eq!(error.to_string(), "test read failure at line 2, column 1");

        let cause = std::error::Error::source(&error).unwrap();
        assert_eq!(cause.to_string(), "test read failure");
    }
}

#[test]
fn reader_resumes_a_partial_character_after_an_io_error() {
    use ReadStep::{Byte, Eoi, Error};
    let mut input = scripted_input(&[
        Byte(0xe5),
        Error(ErrorKind::WouldBlock),
        Byte(0xb9),
        Byte(0x85),
        Eoi,
    ]);
    assert_eq!(
        input.take_while(|_| true).unwrap_err().cause.kind(),
        ErrorKind::WouldBlock
    );
    assert!(input.eat('幅').unwrap());
    assert_eq!(position(input.cursor()), (3, 1, 2));
    assert_eq!(input.peek().unwrap(), None);
}

// Describe physical reads explicitly; reading past the script is a test failure.
#[derive(Clone, Copy)]
enum ReadStep {
    Byte(u8),
    Error(ErrorKind),
    Eoi,
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
            ReadStep::Eoi => Ok(0),
        }
    }
}

fn scripted_input(steps: &[ReadStep]) -> ReaderInput<BufReader<ScriptedReader<'_>>> {
    ReaderInput::new(BufReader::with_capacity(1, ScriptedReader { steps }))
}
