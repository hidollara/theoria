use super::{Input, ReaderInput, StrInput};
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

#[test]
fn peek_leaves_the_next_character_unread() {
    check_inputs!("幅x", |input| {
        assert_eq!(input.peek().unwrap(), Some('幅'));
        assert_eq!(input.finish_capture(), "");
        input.begin_capture();
        input.advance().unwrap();
        input.advance().unwrap();
        assert_eq!(input.finish_capture(), "幅x");
    });
}

#[test]
fn only_characters_advanced_during_a_capture_are_taken() {
    check_inputs!(" 幅x,é", |input| {
        input.advance().unwrap();
        input.begin_capture();
        input.advance().unwrap();
        input.advance().unwrap();
        assert_eq!(input.peek().unwrap(), Some(','));
        assert_eq!(input.finish_capture(), "幅x");
        input.advance().unwrap();
        input.begin_capture();
        input.advance().unwrap();
        assert_eq!(input.finish_capture(), "é");
        assert_eq!(input.peek().unwrap(), None);
        assert_eq!(input.finish_capture(), "");
    });
}

#[test]
fn peek_preserves_the_accumulated_text() {
    check_inputs!("a,b", |input| {
        input.begin_capture();
        input.advance().unwrap();
        assert_eq!(input.peek().unwrap(), Some(','));
        input.advance().unwrap();
        input.advance().unwrap();
        assert_eq!(input.finish_capture(), "a,b");
    });
}

#[test]
fn advancing_at_eoi_keeps_the_capture_empty() {
    for source in ["", "幅\r\n"] {
        check_inputs!(source, |input| {
            input.begin_capture();
            for ch in source.chars() {
                assert_eq!(input.peek().unwrap(), Some(ch));
                input.advance().unwrap();
            }
            assert_eq!(input.finish_capture(), source);
            input.begin_capture();
            input.advance().unwrap();
            assert_eq!(input.finish_capture(), "");
            assert_eq!(input.peek().unwrap(), None);
        });
    }
}

#[test]
fn string_capture_borrows_the_original_slice() {
    let source = String::from("a幅");
    let mut input = StrInput::new(&source);
    input.begin_capture();
    input.advance().unwrap();
    assert_eq!(input.finish_capture(), "a");
    input.begin_capture();
    input.advance().unwrap();
    let Cow::Borrowed(text) = input.finish_capture() else {
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
        input.begin_capture();
        input.advance().unwrap();
        input.finish_capture()
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
        input.begin_capture();
        for ch in characters {
            assert_eq!(input.peek().unwrap(), Some(ch));
            input.advance().unwrap();
        }
        assert_eq!(input.finish_capture(), source, "capacity {capacity}");
        assert_eq!(input.peek().unwrap(), None);
    }
}

#[test]
fn reader_consumes_one_character_when_its_buffer_also_contains_invalid_utf8() {
    let bytes = [b'a', 0xc3, 0xa9, 0xff];
    for capacity in 1..=bytes.len() {
        let mut input = ReaderInput::new(BufReader::with_capacity(capacity, bytes.as_slice()));
        input.begin_capture();
        input.advance().unwrap();
        input.advance().unwrap();
        let error = input.peek().unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
        assert_eq!(input.finish_capture(), "aé");
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
        input.begin_capture();
        input.advance().unwrap();
        input.advance().unwrap();

        let first_error = input.advance().unwrap_err();
        assert_eq!(first_error.kind(), ErrorKind::InvalidData);
        let error = input.peek().unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData, "{reason}");
        let cause = error
            .get_ref()
            .unwrap()
            .downcast_ref::<std::str::Utf8Error>();
        assert_eq!(
            cause,
            Some(&std::str::from_utf8(bytes).unwrap_err()),
            "{reason}"
        );
        assert_eq!(input.finish_capture(), "a\n", "{reason}");
    }
}

#[test]
fn reader_reports_truncated_utf8_as_invalid_data_instead_of_eoi() {
    for character in ['é', '幅', '😀'] {
        let text = character.to_string();
        for length in 1..text.len() {
            let bytes = &text.as_bytes()[..length];
            let mut input = ReaderInput::new(bytes);
            let error = input.peek().unwrap_err();
            assert_eq!(error.kind(), ErrorKind::InvalidData);
            let cause = error
                .get_ref()
                .unwrap()
                .downcast_ref::<std::str::Utf8Error>();
            assert_eq!(cause, Some(&std::str::from_utf8(bytes).unwrap_err()));
            assert_eq!(input.peek().unwrap_err().kind(), ErrorKind::InvalidData);
        }
    }
}

#[test]
fn reader_reuses_the_peeked_character_without_another_read() {
    use ReadStep::Byte;
    // Exactly the bytes of 幅; any further read fails the test.
    let mut input = scripted_input(&[Byte(0xe5), Byte(0xb9), Byte(0x85)]);
    input.begin_capture();
    assert_eq!(input.peek().unwrap(), Some('幅'));
    assert_eq!(input.peek().unwrap(), Some('幅'));
    input.advance().unwrap();
    assert_eq!(input.finish_capture(), "幅");
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
    input.begin_capture();
    input.advance().unwrap();
    assert_eq!(input.finish_capture(), "幅");
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
        input.begin_capture();
        input.advance().unwrap();
        input.advance().unwrap();

        let error = input.peek().unwrap_err();
        assert_eq!(error.kind(), ErrorKind::PermissionDenied);
        assert_eq!(error.to_string(), "test read failure");
        assert_eq!(input.finish_capture(), "a\n");
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
    assert_eq!(input.peek().unwrap_err().kind(), ErrorKind::WouldBlock);
    input.begin_capture();
    input.advance().unwrap();
    assert_eq!(input.finish_capture(), "幅");
    assert_eq!(input.peek().unwrap(), None);
}

#[test]
fn interrupted_text_can_be_taken_after_peeking_at_the_delimiter() {
    use ReadStep::{Byte, Eoi, Error};
    let mut input = scripted_input(&[
        Byte(b'a'),
        Byte(b'b'),
        Error(ErrorKind::WouldBlock),
        Byte(b','),
        Byte(b'c'),
        Eoi,
    ]);
    input.begin_capture();
    input.advance().unwrap();
    input.advance().unwrap();
    let error = input.peek().unwrap_err();
    assert_eq!(error.kind(), ErrorKind::WouldBlock);
    assert_eq!(input.peek().unwrap(), Some(','));
    assert_eq!(input.finish_capture(), "ab");
    input.begin_capture();
    input.advance().unwrap();
    input.advance().unwrap();
    assert_eq!(input.finish_capture(), ",c");
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
