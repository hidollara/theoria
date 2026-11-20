use aition_core::ito::{LexError, ParseError, parse, parse_from_str};
use aition_core::syntax::Module;
use std::io::{BufReader, ErrorKind};

#[test]
fn empty_input_returns_an_empty_module() {
    assert_eq!(parse_from_str("").unwrap(), Module { items: vec![] });
    assert_eq!(parse(&b""[..]).unwrap(), Module { items: vec![] });
}

#[test]
fn comments_and_whitespace_form_an_empty_module() {
    let text = "// 幅\r\n ";
    let reader = BufReader::with_capacity(1, text.as_bytes());

    assert_eq!(parse_from_str(text).unwrap(), Module { items: vec![] });
    assert_eq!(parse(reader).unwrap(), Module { items: vec![] });
}

#[test]
fn both_parse_apis_report_the_closing_paren_after_trivia() {
    let text = "// 幅\r\n )";
    let closing_paren = text.find(')').unwrap();

    let text_error = parse_from_str(text).unwrap_err();
    assert!(
        matches!(text_error, ParseError::Unexpected { cursor, .. } if cursor.offset == closing_paren)
    );

    let reader_error = parse(text.as_bytes()).unwrap_err();
    assert!(
        matches!(reader_error, ParseError::Unexpected { cursor, .. } if cursor.offset == closing_paren)
    );
}

#[test]
fn invalid_utf8_reports_the_reader_position() {
    let error = parse(&b"\n\xff"[..]).unwrap_err();
    let ParseError::Lexical(LexError::Input { cursor, cause }) = error else {
        panic!("expected an input error");
    };
    assert_eq!(cause.kind(), ErrorKind::InvalidData);
    assert_eq!(cursor.offset, 1);
}
#[test]
fn invalid_utf8_after_trivia_is_reported_at_the_failed_read() {
    let input = &b"// note\r\nword\xff"[..];
    let error = aition_core::ito::parse(input).unwrap_err();
    assert!(matches!(error, aition_core::ito::ParseError::Lexical(_)));
    assert_eq!(error.cursor().offset, input.len() - 1);
}
