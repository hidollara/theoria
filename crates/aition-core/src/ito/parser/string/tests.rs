use super::super::ParseError;
use super::string;
use crate::ito::lexer::{self, TokenKind};
use crate::ito::parser::tests::ReadFragment;

#[test]
fn decodes_text_and_escapes_once() {
    for (source, expected) in [
        (r#""""#, ""),
        (r#""幅😀""#, "幅😀"),
        (r#""\"\\\n\r\t""#, "\"\\\n\r\t"),
        (r#""\\u{41}""#, r"\u{41}"),
        (r#""\u{5c}n""#, r"\n"),
    ] {
        let (value, _) = string()
            .read_fragment(&mut lexer::from_str(source))
            .unwrap();
        assert_eq!(
            value.as_ref().map(|(text, _)| text.as_ref()),
            Some(expected),
            "{source}"
        );
    }
}

#[test]
fn unicode_escapes_accept_scalar_values_with_one_to_six_hex_digits() {
    for (source, expected) in [
        (r#""\u{0}""#, "\0"),
        (r#""\u{000041}""#, "A"),
        (r#""\u{5E45}\u{1f600}""#, "幅😀"),
        (r#""\u{D7FF}\u{e000}""#, "\u{d7ff}\u{e000}"),
        (r#""\u{10FFFF}""#, "\u{10ffff}"),
    ] {
        let (value, _) = string()
            .read_fragment(&mut lexer::from_str(source))
            .unwrap();
        assert_eq!(
            value.as_ref().map(|(text, _)| text.as_ref()),
            Some(expected),
            "{source}"
        );
    }
}

#[test]
fn invalid_escapes_report_the_offending_character_or_missing_part() {
    for (escape, offset, expected) in [
        (r"\q", 1, "a valid escape character"),
        (r"\u", 2, "'{'"),
        (r"\u0041", 2, "'{'"),
        (r"\u{}", 3, "a hexadecimal digit"),
        (r"\u{", 3, "a hexadecimal digit"),
        (r"\u{41", 5, "a hexadecimal digit or '}'"),
        (r"\u{g}", 3, "a hexadecimal digit"),
        (r"\u{4 1}", 4, "a hexadecimal digit or '}'"),
        (r"\u{4_1}", 4, "a hexadecimal digit or '}'"),
        (r"\u{0000041}", 9, "'}'"),
        (r"\u{D800}", 3, "a Unicode scalar value"),
        (r"\u{DFFF}", 3, "a Unicode scalar value"),
        (r"\u{110000}", 3, "a Unicode scalar value"),
        (r"\u{FFFFFF}", 3, "a Unicode scalar value"),
    ] {
        let source = format!(" // note\n  \"{escape}\"");
        let error = string()
            .read_fragment(&mut lexer::from_str(&source))
            .err()
            .expect("invalid escape must fail");
        assert!(
            matches!(&error, ParseError::Unexpected { expected: actual, .. }
            if actual == expected),
            "{escape}: {error}"
        );
        assert_eq!(
            error.cursor().offset,
            source.find('"').unwrap() + 1 + offset,
            "{escape}"
        );
        assert_eq!(error.cursor().line, 2, "{escape}");
        assert_eq!(error.cursor().column, 4 + offset, "{escape}");
    }
}

#[test]
fn error_positions_follow_source_characters_in_borrowed_and_streamed_text() {
    for (source, line, column) in [
        ("\"幅😀\\n\\q\"", 1, 7),
        (" // ignored\r\n  \"幅😀\\n\\u{41}\r\né\\q\"", 3, 3),
        ("\"幅\ré\\q\"", 2, 3),
        ("\"幅\né\\q\"", 2, 3),
    ] {
        let error = string()
            .read_fragment(&mut lexer::from_str(source))
            .err()
            .unwrap();
        let cursor = error.cursor();
        assert_eq!(cursor.offset, source.find('q').unwrap(), "{source:?}");
        assert_eq!((cursor.line, cursor.column), (line, column), "{source:?}");
        for capacity in 1..=4 {
            let streamed = string()
                .read_fragment(&mut lexer::from_reader(std::io::BufReader::with_capacity(
                    capacity,
                    source.as_bytes(),
                )))
                .err()
                .unwrap();
            assert_eq!(streamed.cursor(), cursor);
            assert_eq!(streamed.to_string(), error.to_string());
        }
    }
}

#[test]
fn non_string_is_unconsumed_and_unterminated_string_stays_a_lexical_error() {
    let mut stream = lexer::from_str("name");
    let (value, mut tokens) = string().read_fragment(&mut stream).unwrap();
    assert_eq!(value, None);
    let token = tokens.next_if(|_| true).unwrap().unwrap();
    assert!(matches!(token.kind, TokenKind::Word(text) if text == "name"));

    let error = string()
        .read_fragment(&mut lexer::from_str(r#""unfinished"#))
        .err()
        .unwrap();
    assert!(matches!(error, ParseError::Lexical(_)));
}
