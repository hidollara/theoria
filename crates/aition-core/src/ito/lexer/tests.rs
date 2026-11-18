use super::{Cursor, LexError, Lexer, Symbol, Token};
use crate::input::{ReaderInput, StrInput};
use std::{
    borrow::Cow,
    io::{self, BufReader, Read},
};

fn next_token<'s>(stream: &mut impl Iterator<Item = Result<Token<'s>, LexError>>) -> Token<'s> {
    stream.next().expect("expected a token").unwrap()
}

fn word_text<'s>(token: Token<'s>) -> Cow<'s, str> {
    let Token::Word { text, .. } = token else {
        panic!("expected a word, got {:?}", token);
    };
    text
}

#[test]
fn advancing_a_cursor_copy_preserves_utf8_and_crlf_position_rules() {
    let start = Cursor::default();
    let wide = start.advanced(['幅']);
    assert_eq!((wide.offset, wide.line, wide.column), (3, 1, 2));
    let cr = wide.advanced(['\r']);
    let lf = cr.advanced(['\n']);
    assert_eq!((cr.offset, cr.line, cr.column), (4, 2, 1));
    assert_eq!((lf.offset, lf.line, lf.column), (5, 2, 1));
    assert_eq!(start, Cursor::default());
    assert_eq!((wide.offset, wide.line, wide.column), (3, 1, 2));
}

#[test]
fn words_are_not_validated_as_identifiers() {
    // Digits, Unicode, and non-symbol punctuation are still one word.
    // Whether they form an identifier is the parser's decision.
    for source in ["hello", "_", "123abc", "123", "幅", "aé", "foo-bar", "a#b"] {
        let token = next_token(&mut Lexer::new(&mut StrInput::new(source)));
        assert_eq!(word_text(token), source);
    }
}

#[test]
fn each_symbol_separates_adjacent_words() {
    for (character, symbol) in [
        ('(', Symbol::OpenParen),
        (')', Symbol::CloseParen),
        ('[', Symbol::OpenBracket),
        (']', Symbol::CloseBracket),
        ('{', Symbol::OpenBrace),
        ('}', Symbol::CloseBrace),
        ('<', Symbol::OpenAngle),
        ('>', Symbol::CloseAngle),
        ('.', Symbol::Dot),
        (',', Symbol::Comma),
        (':', Symbol::Colon),
        ('=', Symbol::Equal),
        ('@', Symbol::At),
        ('|', Symbol::Pipe),
        ('&', Symbol::Ampersand),
        ('?', Symbol::Question),
        ('*', Symbol::Asterisk),
        ('/', Symbol::Slash),
    ] {
        let source = format!("幅\r\n{character}right");
        let mut input = StrInput::new(&source);
        let mut tokens = Lexer::new(&mut input);
        assert_eq!(word_text(next_token(&mut tokens)), "幅");
        let punctuation = next_token(&mut tokens);
        let start = punctuation.cursor();
        assert_eq!(punctuation, Token::Symbol { symbol, start });
        assert_eq!((start.offset, start.line, start.column), (5, 2, 1));
        assert_eq!(word_text(next_token(&mut tokens)), "right");
    }
}

#[test]
fn string_tokens_borrow_the_undecoded_text_between_quotes() {
    for (source, body) in [
        (r#""""#, ""),
        (r#""a\"b""#, r#"a\"b"#),
        (r#""a\\""#, r#"a\\"#),
        (r#""\q""#, r#"\q"#),
        (r#""// 幅""#, "// 幅"),
        ("\"first\nsecond\"", "first\nsecond"),
    ] {
        let token = next_token(&mut Lexer::new(&mut StrInput::new(source)));
        let Token::String(token) = token else {
            panic!("expected a string")
        };
        assert_eq!(token.start, Cursor::default());
        assert_eq!(token.content_start().offset, 1);
        let Cow::Borrowed(content) = token.content else {
            panic!("expected borrowed string content")
        };
        assert_eq!(content, body);
        assert_eq!(content.as_ptr(), source[1..].as_ptr());
    }
}

#[test]
fn positions_after_a_multiline_string_count_utf8_and_crlf() {
    let mut input = StrInput::new("  \"幅\r\n😀\"  .");
    let mut tokens = Lexer::new(&mut input);
    let quoted = next_token(&mut tokens);
    let start = quoted.cursor();
    assert_eq!((start.offset, start.line, start.column), (2, 1, 3));
    let dot = next_token(&mut tokens).cursor();
    assert_eq!((dot.offset, dot.line, dot.column), (15, 2, 5));
}

#[test]
fn lexer_cursor_advances_over_trailing_trivia_at_eof() {
    for newline in ["\n", "\r", "\r\n"] {
        let source = format!("幅// \"quoted\" // comment{newline}  \"é\" // end");
        let mut input = StrInput::new(&source);
        let mut tokens = Lexer::new(&mut input);
        let first = next_token(&mut tokens);
        assert_eq!(first.cursor(), Cursor::default());
        let before_string = tokens.cursor();

        let string = next_token(&mut tokens);
        let Token::String(ref literal) = string else {
            panic!("expected a string")
        };
        let start = literal.content_start();
        assert_eq!(
            (start.offset, start.line, start.column),
            (source.find('é').unwrap(), 2, 4)
        );
        assert_eq!(before_string.offset, "幅".len());
        assert_eq!(
            &source[before_string.offset..literal.start.offset],
            format!("// \"quoted\" // comment{newline}  ")
        );

        let before_eof = tokens.cursor();
        assert!(tokens.next().is_none());
        assert_eq!(
            &source[before_eof.offset..tokens.cursor().offset],
            " // end"
        );
        assert_eq!(tokens.cursor().offset, source.len());
    }
}

#[test]
fn peeking_preserves_the_committed_cursor_until_next_or_commit() {
    let mut input = StrInput::new(" a b  ");
    let mut tokens = Lexer::new(&mut input);
    assert_eq!(tokens.next_start().unwrap().offset, 1);
    assert_eq!(tokens.cursor().offset, 0);
    assert_eq!(word_text(next_token(&mut tokens)), "a");
    assert_eq!(tokens.cursor().offset, 2);

    assert_eq!(tokens.peek().unwrap().unwrap().cursor().offset, 3);
    assert_eq!(tokens.next_start().unwrap().offset, 3);
    assert_eq!(tokens.cursor().offset, 2);
    assert_eq!(word_text(tokens.commit()), "b");
    assert_eq!(tokens.cursor().offset, 4);

    assert!(tokens.peek().unwrap().is_none());
    assert_eq!(tokens.next_start().unwrap().offset, 6);
    assert_eq!(tokens.cursor().offset, 4);
    assert!(tokens.next().is_none());
    assert_eq!(tokens.cursor().offset, 6);
}

#[test]
fn empty_input_stays_at_eof() {
    let mut input = StrInput::new("");
    let mut tokens = Lexer::new(&mut input);
    assert!(tokens.next().is_none());
    assert!(tokens.next().is_none());
    assert_eq!(tokens.cursor(), Cursor::default());
}

#[test]
fn unterminated_strings_stay_at_the_opening_quote() {
    for source in [
        r#"""#,        // Just the opening quote.
        r#""hello"#,   // End of input in ordinary text.
        r#""hello\"#,  // End of input after an escape introducer.
        r#""hello\""#, // Escaped quote does not close the string.
        "\"first\nsecond",
    ] {
        assert_unterminated(Lexer::new(&mut StrInput::new(source)));
        assert_unterminated(Lexer::new(&mut ReaderInput::new(BufReader::with_capacity(
            1,
            source.as_bytes(),
        ))));
    }

    fn assert_unterminated<'source>(
        mut lexer: impl Iterator<Item = Result<Token<'source>, LexError>>,
    ) {
        let error = lexer.next().unwrap().unwrap_err();
        let LexError::UnterminatedString { opening } = error else {
            panic!("expected an unterminated string, got {error:?}");
        };
        assert_eq!((opening.offset, opening.line, opening.column), (0, 1, 1));

        assert!(
            matches!(lexer.next(), Some(Err(LexError::UnterminatedString { opening: again })) if again == opening)
        );
    }
}

#[test]
fn unterminated_string_points_to_its_opening_after_other_tokens() {
    let mut input = StrInput::new("幅\n\"unfinished\ntext");
    let mut lexer = Lexer::new(&mut input);
    assert_eq!(word_text(next_token(&mut lexer)), "幅");

    let error = lexer.next().unwrap().unwrap_err();
    let LexError::UnterminatedString { opening } = error else {
        panic!("expected an unterminated string, got {error:?}");
    };
    assert_eq!((opening.offset, opening.line, opening.column), (4, 2, 1));
    assert_eq!(error.cursor(), &opening);
}

#[test]
fn invalid_input_inside_a_string_is_not_an_unterminated_string() {
    // Includes truncated UTF-8: only a clean character boundary can end the input.
    for source in [&[b'"', 0xff][..], &[b'"', 0xe5]] {
        let mut input = ReaderInput::new(source);
        let mut lexer = Lexer::new(&mut input);
        let error = lexer.next().unwrap().unwrap_err();
        assert_eq!(error.cursor().offset, 1);
        let LexError::Input { cursor, cause } = error else {
            panic!("expected an input error, got {error:?}");
        };
        assert_eq!(cause.kind(), std::io::ErrorKind::InvalidData);
        assert_eq!(cursor.offset, 1);

        assert_eq!(lexer.next().unwrap().unwrap_err().cursor(), &cursor);
    }
}

#[test]
fn invalid_utf8_cannot_close_a_token_that_needs_lookahead() {
    for text in ["abc", " ", "/", "//note"] {
        for suffix in [&b"\xff"[..], &b"\xe5\xb9"[..], &b"\xe5\xb9x"[..]] {
            let bytes = [text.as_bytes(), suffix].concat();
            for capacity in 1..=bytes.len() {
                let mut input =
                    ReaderInput::new(BufReader::with_capacity(capacity, bytes.as_slice()));
                let mut stream = Lexer::new(&mut input);
                let LexError::Input { cursor, cause } = stream.next().unwrap().unwrap_err() else {
                    panic!("expected a UTF-8 error");
                };
                assert_eq!(cursor.offset, text.len());
                assert_eq!(cause.kind(), std::io::ErrorKind::InvalidData);

                assert_eq!(stream.next().unwrap().unwrap_err().cursor(), &cursor);
            }
        }
    }
}

#[test]
fn a_known_token_boundary_before_invalid_utf8_is_preserved() {
    let mut input = ReaderInput::new(&b"abc:\xff"[..]);
    let mut stream = Lexer::new(&mut input);
    assert_eq!(word_text(next_token(&mut stream)), "abc");
    assert!(matches!(
        next_token(&mut stream),
        Token::Symbol {
            symbol: Symbol::Colon,
            ..
        }
    ));
    assert_eq!(stream.next().unwrap().unwrap_err().cursor().offset, 4);
}

#[test]
fn malformed_utf8_at_start_blocks_following_tokens() {
    let mut input = ReaderInput::new(&b"\xff\nvalid"[..]);
    let mut lexer = Lexer::new(&mut input);
    let LexError::Input { cursor, cause } = lexer.next().unwrap().unwrap_err() else {
        panic!("expected an input error");
    };
    assert_eq!(cursor, Cursor::default());
    assert_eq!(cause.kind(), std::io::ErrorKind::InvalidData);
    assert_eq!(lexer.next().unwrap().unwrap_err().cursor(), &cursor);
}

#[test]
fn streamed_tokens_own_their_text() {
    let token = {
        let source = String::from(r#""幅""#);
        next_token(&mut Lexer::new(&mut ReaderInput::new(source.as_bytes())))
    };
    let Token::String(token) = token else {
        panic!("expected a string")
    };
    assert!(matches!(token.content, Cow::Owned(content) if content == "幅"));
}

#[test]
fn reader_buffer_boundaries_do_not_change_lexing() {
    for source in [
        r#"left"幅\"x"right"#,
        "// \"幅\"\r\nnext",
        " \t\u{2003}\r\n",
        "<>[]{}()*,.:=@|&?/",
    ] {
        let expected = Lexer::new(&mut StrInput::new(source))
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        for capacity in 1..=source.len() {
            let reader = BufReader::with_capacity(capacity, source.as_bytes());
            let actual = Lexer::new(&mut ReaderInput::new(reader))
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            assert_eq!(actual, expected, "{source:?}, buffer capacity {capacity}");
        }
    }
}

/// Return WouldBlock before every byte and once before EOF.
struct PausingReader<'a> {
    bytes: &'a [u8],
    pause: bool,
}

impl Read for PausingReader<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        if self.pause {
            self.pause = false;
            return Err(io::ErrorKind::WouldBlock.into());
        }
        let Some((&byte, rest)) = self.bytes.split_first() else {
            return Ok(0);
        };
        output[0] = byte;
        self.bytes = rest;
        self.pause = true;
        Ok(1)
    }
}

#[test]
fn would_block_preserves_the_partial_token_and_its_start() {
    for source in [
        "幅xyz \r\n\t",
        r#""a\"幅\\b"/x"#,
        "(// note幅\r\n/)",
        "// last comment",
        "/",
    ] {
        let reader = PausingReader {
            bytes: source.as_bytes(),
            pause: true,
        };
        let mut input = ReaderInput::new(BufReader::new(reader));
        let mut stream = Lexer::new(&mut input);
        let mut str_input = StrInput::new(source);
        let mut uninterrupted = Lexer::new(&mut str_input);
        let mut pauses = 0;
        loop {
            match stream.next().transpose() {
                Err(LexError::Input { cursor, cause })
                    if cause.kind() == io::ErrorKind::WouldBlock =>
                {
                    assert_eq!(cursor.offset, source.floor_char_boundary(pauses));
                    assert_eq!(stream.cursor(), uninterrupted.cursor());
                    pauses += 1;
                }
                result => {
                    let token = result.unwrap();
                    assert_eq!(
                        token,
                        uninterrupted.next().transpose().unwrap(),
                        "{source:?}"
                    );
                    if token.is_none() {
                        break;
                    }
                }
            }
        }
        assert_eq!(pauses, source.len() + 1);
    }
}

#[test]
fn a_streamed_word_starts_after_a_long_comment() {
    let source = format!("//{}\nnext", "幅".repeat(4096));
    let mut input = ReaderInput::new(BufReader::with_capacity(3, source.as_bytes()));
    let mut tokens = Lexer::new(&mut input);
    let word = next_token(&mut tokens);
    let start = word.cursor();
    assert!(matches!(&word, Token::Word { text, .. } if text == "next"));
    assert_eq!(start.offset, source.len() - "next".len());
    assert_eq!(tokens.cursor().offset, source.len());
}
