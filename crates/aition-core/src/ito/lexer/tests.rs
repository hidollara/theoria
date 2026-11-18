use super::{
    LexError, Symbol, Token, TokenKind, TokenStream, TokenStreamItem, from_reader, from_str,
};
use TokenKind::{LineComment, String as StringToken, Whitespace, Word};
use std::{borrow::Cow, io::BufReader};

fn tokens(source: &str) -> Vec<Token<'_>> {
    read_tokens(from_str(source))
}

fn read_tokens<'s>(mut stream: impl TokenStream<'s>) -> Vec<Token<'s>> {
    let mut tokens = Vec::new();
    while let TokenStreamItem::Token(token) = stream.next().unwrap() {
        tokens.push(token);
    }
    tokens
}

fn next_token<'s>(stream: &mut impl TokenStream<'s>) -> Token<'s> {
    let TokenStreamItem::Token(token) = stream.next().unwrap() else {
        panic!("expected a token");
    };
    token
}

fn kinds(source: &str) -> Vec<TokenKind<'_>> {
    tokens(source).into_iter().map(|token| token.kind).collect()
}

fn single_token(source: &str) -> Token<'_> {
    let mut lexer = from_str(source);
    let token = next_token(&mut lexer);
    assert!(matches!(lexer.next().unwrap(), TokenStreamItem::Eoi(_)));
    token
}

#[test]
fn content_start_excludes_delimiters_without_changing_the_full_span() {
    for (source, offset, column) in [
        (r#""幅😀""#, 1, 2),
        (r#""""#, 1, 2),
        ("//幅😀", 2, 3),
        ("//", 2, 3),
        ("幅", 0, 1),
        (" \t", 0, 1),
        ("(", 0, 1),
    ] {
        for token in [
            single_token(source),
            next_token(&mut from_reader(source.as_bytes())),
        ] {
            let start = token.content_start();
            assert_eq!(
                (start.offset, start.line, start.column),
                (offset, 1, column),
                "{source:?}"
            );
            assert_eq!(token.start.offset..token.end.offset, 0..source.len());
            assert_eq!(token.start, super::Cursor::default());
        }
    }
    let source = "// heading\r\n  \"幅\"";
    let literal = tokens(source).swap_remove(2);
    let content = literal.content_start();
    assert_eq!(content.offset, source.find('幅').unwrap());
    assert_eq!((content.line, content.column), (2, 4));
}

#[test]
fn advancing_a_cursor_copy_preserves_utf8_and_crlf_position_rules() {
    let start = super::Cursor::default();
    let wide = start.advanced('幅');
    assert_eq!((wide.offset, wide.line, wide.column), (3, 1, 2));
    let cr = wide.advanced('\r');
    let lf = cr.advanced('\n');
    assert_eq!((cr.offset, cr.line, cr.column), (4, 2, 1));
    assert_eq!((lf.offset, lf.line, lf.column), (5, 2, 1));
    assert_eq!(start, super::Cursor::default());
    assert_eq!((wide.offset, wide.line, wide.column), (3, 1, 2));
}

#[test]
fn words_are_not_validated_as_identifiers() {
    // Digits, Unicode, and non-symbol punctuation are still one word.
    // Whether they form an identifier is the parser's decision.
    for source in ["hello", "_", "123abc", "123", "幅", "aé", "foo-bar", "a#b"] {
        assert_eq!(kinds(source), [Word(source.into())], "{source:?}");
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
        let source = format!("left{character}right");
        assert_eq!(
            kinds(&source),
            [
                Word("left".into()),
                TokenKind::Symbol(symbol),
                Word("right".into()),
            ],
            "{source:?}",
        );
    }
}

#[test]
fn adjacent_string_and_words_need_no_whitespace() {
    assert_eq!(
        kinds(r#"left"middle"right"#),
        [
            Word("left".into()),
            StringToken("middle".into()),
            Word("right".into()),
        ],
    );
}

#[test]
fn consecutive_whitespace_is_preserved_as_one_token() {
    assert_eq!(
        kinds("left \t\u{2003}\r\nright"),
        [
            Word("left".into()),
            Whitespace(" \t\u{2003}\r\n".into()),
            Word("right".into()),
        ],
    );
}

#[test]
fn string_payload_excludes_quotes_but_its_span_includes_them() {
    for (source, body) in [(r#""hello""#, "hello"), (r#""""#, ""), (r#""幅""#, "幅")] {
        let token = single_token(source);
        assert_eq!(token.kind, StringToken(body.into()), "{source:?}");
        assert_eq!(token.start.offset..token.end.offset, 0..source.len());
    }
}

#[test]
fn comment_payload_excludes_slashes_but_keeps_body_spaces() {
    for (source, body) in [("// hello  ", " hello  "), ("//", ""), ("// 幅", " 幅")] {
        let token = single_token(source);
        assert_eq!(token.kind, LineComment(body.into()), "{source:?}");
        assert_eq!(token.start.offset..token.end.offset, 0..source.len());
    }
}

#[test]
fn comment_ends_before_the_line_ending() {
    for newline in ["\n", "\r", "\r\n"] {
        let source = format!("left// note{newline}right");
        assert_eq!(
            kinds(&source),
            [
                Word("left".into()),
                LineComment(" note".into()),
                Whitespace(newline.into()),
                Word("right".into()),
            ],
            "line ending {newline:?}",
        );
        let comment = &tokens(&source)[1];
        assert_eq!(comment.start.offset..comment.end.offset, 4..11);
    }
}

#[test]
fn slashes_inside_a_string_do_not_start_a_comment() {
    assert_eq!(kinds(r#""// text""#), [StringToken("// text".into())]);
}

#[test]
fn quotes_and_slashes_inside_a_comment_are_plain_text() {
    assert_eq!(
        kinds(r#"// "text" // more"#),
        [LineComment(r#" "text" // more"#.into())],
    );
}

#[test]
fn escapes_affect_the_closing_quote_without_being_decoded() {
    for (case, source, body) in [
        ("escaped quote", r#""a\"b""#, r#"a\"b"#),
        ("escaped backslash", r#""a\\""#, r#"a\\"#),
        ("newline escape stays text", r#""a\nb""#, r#"a\nb"#),
        ("unknown escape stays text", r#""\q""#, r#"\q"#),
    ] {
        assert_eq!(kinds(source), [StringToken(body.into())], "{case}");
    }
}

#[test]
fn actual_newlines_inside_a_string_are_preserved() {
    assert_eq!(
        kinds("\"first\nsecond\""),
        [StringToken("first\nsecond".into())],
    );
}

#[test]
fn token_positions_include_delimiters_and_count_unicode_columns() {
    let source = "幅 \"é\"\r\nx";
    let actual: Vec<_> = tokens(source)
        .into_iter()
        .map(|token| {
            (
                &source[token.start.offset..token.end.offset],
                token.start.offset..token.end.offset,
                (token.start.line, token.start.column),
                (token.end.line, token.end.column),
            )
        })
        .collect();

    // Full spelling, byte range, start and end as one-based (line, column).
    assert_eq!(
        actual,
        [
            ("幅", 0..3, (1, 1), (1, 2)),
            (" ", 3..4, (1, 2), (1, 3)),
            (r#""é""#, 4..8, (1, 3), (1, 6)),
            ("\r\n", 8..10, (1, 6), (2, 1)),
            ("x", 10..11, (2, 1), (2, 2)),
        ],
    );
}

#[test]
fn eoi_repeats_at_the_final_cursor() {
    fn check<'s>(mut lexer: impl TokenStream<'s>, expected: (usize, usize, usize)) {
        let end = loop {
            if let TokenStreamItem::Eoi(end) = lexer.next().unwrap() {
                break end;
            }
        };
        assert_eq!((end.offset, end.line, end.column), expected);
        for _ in 0..3 {
            assert_eq!(lexer.next().unwrap(), TokenStreamItem::Eoi(end));
        }
    }

    for (source, line, column) in [("", 1, 1), ("f() ", 1, 5), ("幅\r\n// 尾", 2, 5)] {
        let expected = (source.len(), line, column);
        check(from_str(source), expected);
        check(
            from_reader(BufReader::with_capacity(1, source.as_bytes())),
            expected,
        );
    }
}

#[test]
fn unterminated_strings_report_the_opening_quote_then_reach_eoi() {
    for source in [
        r#"""#,        // Just the opening quote.
        r#""hello"#,   // End of input in ordinary text.
        r#""hello\"#,  // End of input after an escape introducer.
        r#""hello\""#, // Escaped quote does not close the string.
        "\"first\nsecond",
    ] {
        assert_unterminated(from_str(source));
        assert_unterminated(from_reader(BufReader::with_capacity(1, source.as_bytes())));
    }

    fn assert_unterminated<'source>(mut lexer: impl TokenStream<'source>) {
        let error = lexer.next().unwrap_err();
        let LexError::UnterminatedString { opening } = error else {
            panic!("expected an unterminated string, got {error:?}");
        };
        assert_eq!((opening.offset, opening.line, opening.column), (0, 1, 1));
        assert!(matches!(lexer.next().unwrap(), TokenStreamItem::Eoi(_)));
    }
}

#[test]
fn unterminated_string_points_to_its_opening_after_other_tokens() {
    let mut lexer = from_str("幅\n\"unfinished\ntext");
    assert_eq!(next_token(&mut lexer).kind, Word("幅".into()));
    assert_eq!(next_token(&mut lexer).kind, Whitespace("\n".into()));

    let error = lexer.next().unwrap_err();
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
        let mut lexer = from_reader(source);
        let error = lexer.next().unwrap_err();
        assert_eq!(error.cursor().offset, 1);
        let LexError::Input(error) = error else {
            panic!("expected an input error, got {error:?}");
        };
        assert_eq!(error.cause.kind(), std::io::ErrorKind::InvalidData);
        assert_eq!(error.start.offset, 1);
        assert_eq!(lexer.next().unwrap_err().cursor(), &error.start);
    }
}

#[test]
fn invalid_utf8_cannot_close_a_token_that_needs_lookahead() {
    for text in ["abc", " ", "/", "//note"] {
        for suffix in [&b"\xff"[..], &b"\xe5\xb9"[..], &b"\xe5\xb9x"[..]] {
            let bytes = [text.as_bytes(), suffix].concat();
            for capacity in 1..=bytes.len() {
                let mut stream = from_reader(BufReader::with_capacity(capacity, bytes.as_slice()));
                for _ in 0..2 {
                    let LexError::Input(error) = stream.next().unwrap_err() else {
                        panic!("expected a UTF-8 error");
                    };
                    assert_eq!(error.start.offset, text.len());
                    assert_eq!(error.cause.kind(), std::io::ErrorKind::InvalidData);
                }
            }
        }
    }
}

#[test]
fn a_known_token_boundary_before_invalid_utf8_is_preserved() {
    let mut stream = from_reader(&b"abc:\xff"[..]);
    assert_eq!(next_token(&mut stream).kind, Word("abc".into()));
    assert_eq!(
        next_token(&mut stream).kind,
        TokenKind::Symbol(Symbol::Colon)
    );
    assert_eq!(stream.next().unwrap_err().cursor().offset, 4);
}

#[test]
fn malformed_utf8_at_start_blocks_following_tokens() {
    let mut lexer = from_reader(&b"\xff\nvalid"[..]);
    for _ in 0..2 {
        let LexError::Input(error) = lexer.next().unwrap_err() else {
            panic!("expected an input error");
        };
        assert_eq!(error.start, super::Cursor::default());
        assert_eq!(error.cause.kind(), std::io::ErrorKind::InvalidData);
    }
}

#[test]
fn text_payloads_borrow_their_exact_source_slice() {
    // Source spelling and payload byte range (excluding delimiters).
    for (source, payload) in [
        ("word", 0..4),
        (r#""幅""#, 1..4),
        ("// body", 2..7),
        (" \t", 0..2),
    ] {
        let source = source.to_owned();
        let token = single_token(&source);
        let Cow::Borrowed(text) = text_payload(&token.kind) else {
            panic!("string input should borrow its payload");
        };
        let expected = &source[payload];
        assert_eq!(*text, expected);
        assert_eq!(text.as_ptr(), expected.as_ptr());
    }
}

#[test]
fn streamed_payloads_survive_dropping_the_source() {
    for (source, expected) in [
        ("word", "word"),
        (r#""幅""#, "幅"),
        ("// body", " body"),
        (" \t", " \t"),
    ] {
        let token = {
            let source = source.to_owned();
            let mut lexer = from_reader(source.as_bytes());
            let token = next_token(&mut lexer);
            assert!(matches!(lexer.next().unwrap(), TokenStreamItem::Eoi(_)));
            token
        };
        assert!(matches!(text_payload(&token.kind), Cow::Owned(_)));
        assert_eq!(text_payload(&token.kind), expected);
    }
}

#[test]
fn reader_buffer_boundaries_do_not_change_lexing() {
    for source in [
        r#"left"幅\"x"right"#,
        "// \"幅\"\r\nnext",
        " \t\u{2003}\r\n",
        "<>[]{}()*,.:=@|&?/",
    ] {
        let expected = tokens(source);
        for capacity in 1..=source.len() {
            let reader = BufReader::with_capacity(capacity, source.as_bytes());
            let actual = read_tokens(from_reader(reader));
            assert_eq!(actual, expected, "{source:?}, buffer capacity {capacity}");
        }
    }
}

#[test]
fn a_token_can_span_many_reader_buffers() {
    let body = "幅".repeat(4096);
    let source = format!("//{body}\nnext");
    let mut lexer = from_reader(BufReader::with_capacity(3, source.as_bytes()));

    let comment = next_token(&mut lexer);
    assert_eq!(comment.kind, LineComment(body.clone().into()));
    assert_eq!(comment.start.offset..comment.end.offset, 0..2 + body.len());
    assert_eq!(next_token(&mut lexer).kind, Whitespace("\n".into()));
    assert_eq!(next_token(&mut lexer).kind, Word("next".into()));
    assert!(matches!(lexer.next().unwrap(), TokenStreamItem::Eoi(_)));
}

fn text_payload<'a, 'source>(kind: &'a TokenKind<'source>) -> &'a Cow<'source, str> {
    match kind {
        Word(text) | StringToken(text) | LineComment(text) | Whitespace(text) => text,
        TokenKind::Symbol(_) => panic!("expected a text payload"),
    }
}
