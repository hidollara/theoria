use super::{LexError, Lexeme, LexemeIterator, LexemeKind, Symbol, from_reader, from_str};
use LexemeKind::{LineComment, String as StringLexeme, Whitespace, Word};
use std::{borrow::Cow, io::BufReader};

fn lexemes(source: &str) -> Vec<Lexeme<'_>> {
    from_str(source).collect::<Result<_, _>>().unwrap()
}

fn kinds(source: &str) -> Vec<LexemeKind<'_>> {
    lexemes(source)
        .into_iter()
        .map(|lexeme| lexeme.kind)
        .collect()
}

fn single_lexeme(source: &str) -> Lexeme<'_> {
    let mut lexer = from_str(source);
    let lexeme = lexer.next().unwrap().unwrap();
    assert!(lexer.next().is_none(), "expected one lexeme for {source:?}");
    lexeme
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
        (';', Symbol::Semicolon),
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
                LexemeKind::Symbol(symbol),
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
            StringLexeme("middle".into()),
            Word("right".into()),
        ],
    );
}

#[test]
fn consecutive_whitespace_is_preserved_as_one_lexeme() {
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
        let lexeme = single_lexeme(source);
        assert_eq!(lexeme.kind, StringLexeme(body.into()), "{source:?}");
        assert_eq!(lexeme.span(), 0..source.len());
    }
}

#[test]
fn comment_payload_excludes_slashes_but_keeps_body_spaces() {
    for (source, body) in [("// hello  ", " hello  "), ("//", ""), ("// 幅", " 幅")] {
        let lexeme = single_lexeme(source);
        assert_eq!(lexeme.kind, LineComment(body.into()), "{source:?}");
        assert_eq!(lexeme.span(), 0..source.len());
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
        assert_eq!(lexemes(&source)[1].span(), 4..11);
    }
}

#[test]
fn slashes_inside_a_string_do_not_start_a_comment() {
    assert_eq!(kinds(r#""// text""#), [StringLexeme("// text".into())]);
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
        assert_eq!(kinds(source), [StringLexeme(body.into())], "{case}");
    }
}

#[test]
fn actual_newlines_inside_a_string_are_preserved() {
    assert_eq!(
        kinds("\"first\nsecond\""),
        [StringLexeme("first\nsecond".into())],
    );
}

#[test]
fn token_positions_include_delimiters_and_count_unicode_columns() {
    let source = "幅 \"é\"\r\nx";
    let actual: Vec<_> = lexemes(source)
        .into_iter()
        .map(|lexeme| {
            (
                &source[lexeme.span()],
                lexeme.span(),
                (lexeme.start.line, lexeme.start.column),
                (lexeme.end.line, lexeme.end.column),
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
fn eof_is_stable_for_empty_and_consumed_input() {
    for source in ["", "word"] {
        let mut lexer = from_str(source);
        if !source.is_empty() {
            assert_eq!(lexer.next().unwrap().unwrap().kind, Word("word".into()));
        }
        assert!(lexer.next().is_none());
        assert!(lexer.next().is_none());
    }
}

#[test]
fn unterminated_strings_report_the_opening_quote_then_stop() {
    for source in [
        r#"""#,        // Just the opening quote.
        r#""hello"#,   // EOF in ordinary text.
        r#""hello\"#,  // EOF after an escape introducer.
        r#""hello\""#, // Escaped quote does not close the string.
        "\"first\nsecond",
    ] {
        assert_unterminated(from_str(source));
        assert_unterminated(from_reader(BufReader::with_capacity(1, source.as_bytes())));
    }

    fn assert_unterminated<'source>(mut lexer: impl LexemeIterator<'source>) {
        let error = lexer.next().unwrap().unwrap_err();
        let LexError::UnterminatedString { opening } = error else {
            panic!("expected an unterminated string, got {error:?}");
        };
        assert_eq!((opening.offset, opening.line, opening.column), (0, 1, 1));
        assert_eq!(error.cursor(), &opening);
        assert!(lexer.next().is_none());
        assert!(lexer.next().is_none());
    }
}

#[test]
fn unterminated_string_points_to_its_opening_after_other_lexemes() {
    let mut lexer = from_str("幅\n\"unfinished\ntext");
    assert_eq!(lexer.next().unwrap().unwrap().kind, Word("幅".into()));
    assert_eq!(lexer.next().unwrap().unwrap().kind, Whitespace("\n".into()));

    let error = lexer.next().unwrap().unwrap_err();
    let LexError::UnterminatedString { opening } = error else {
        panic!("expected an unterminated string, got {error:?}");
    };
    assert_eq!((opening.offset, opening.line, opening.column), (4, 2, 1));
    assert_eq!(error.cursor(), &opening);
    assert!(lexer.next().is_none());
}

#[test]
fn invalid_input_inside_a_string_is_not_an_unterminated_string() {
    // Includes truncated UTF-8: only a clean character boundary can be EOF.
    for source in [&[b'"', 0xff][..], &[b'"', 0xe5]] {
        let mut lexer = from_reader(source);
        let error = lexer.next().unwrap().unwrap_err();
        assert_eq!(error.cursor().offset, 1);
        let LexError::Input(error) = error else {
            panic!("expected an input error, got {error:?}");
        };
        assert_eq!(error.cause.kind(), std::io::ErrorKind::InvalidData);
        assert_eq!(error.start.offset, 1);
        assert!(lexer.next().is_none());
        assert!(lexer.next().is_none());
    }
}

#[test]
fn input_failure_stops_before_following_valid_lexemes() {
    let mut lexer = from_reader(&b"\xff valid"[..]);
    assert!(matches!(lexer.next(), Some(Err(LexError::Input(_)))));
    assert!(lexer.next().is_none());
    assert!(lexer.next().is_none());
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
        let lexeme = single_lexeme(&source);
        let Cow::Borrowed(text) = text_payload(&lexeme.kind) else {
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
        let lexeme = {
            let source = source.to_owned();
            let mut lexer = from_reader(source.as_bytes());
            let lexeme = lexer.next().unwrap().unwrap();
            assert!(lexer.next().is_none());
            lexeme
        };
        assert!(matches!(text_payload(&lexeme.kind), Cow::Owned(_)));
        assert_eq!(text_payload(&lexeme.kind), expected);
    }
}

#[test]
fn reader_buffer_boundaries_do_not_change_lexing() {
    for source in [
        r#"left"幅\"x"right"#,
        "// \"幅\"\r\nnext",
        " \t\u{2003}\r\n",
        "<>[]{}()*,.;:=@|&?/",
    ] {
        let expected = lexemes(source);
        for capacity in 1..=source.len() {
            let reader = BufReader::with_capacity(capacity, source.as_bytes());
            let actual = from_reader(reader).collect::<Result<Vec<_>, _>>().unwrap();
            assert_eq!(actual, expected, "{source:?}, buffer capacity {capacity}");
        }
    }
}

#[test]
fn a_lexeme_can_span_many_reader_buffers() {
    let body = "幅".repeat(4096);
    let source = format!("//{body}\nnext");
    let mut lexer = from_reader(BufReader::with_capacity(3, source.as_bytes()));

    let comment = lexer.next().unwrap().unwrap();
    assert_eq!(comment.kind, LineComment(body.clone().into()));
    assert_eq!(comment.span(), 0..2 + body.len());
    assert_eq!(lexer.next().unwrap().unwrap().kind, Whitespace("\n".into()));
    assert_eq!(lexer.next().unwrap().unwrap().kind, Word("next".into()));
    assert!(lexer.next().is_none());
}

fn text_payload<'a, 'source>(kind: &'a LexemeKind<'source>) -> &'a Cow<'source, str> {
    match kind {
        Word(text) | StringLexeme(text) | LineComment(text) | Whitespace(text) => text,
        LexemeKind::Symbol(_) => panic!("expected a text payload"),
    }
}
