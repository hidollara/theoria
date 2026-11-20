use aition_core::{
    input::Cursor,
    ito::{LexError, ParseError, parse, parse_from_str},
    syntax::ast::TriviaKind,
};
use std::io::{BufReader, ErrorKind};

#[test]
fn empty_module_has_an_empty_span() {
    let module = parse_from_str("").unwrap();
    assert_eq!(parse(&b""[..]).unwrap(), module);
    let source = module.source.unwrap();
    assert_eq!(source.start, Cursor::default());
    assert_eq!(source.end, source.start);
    assert!(source.trivia.is_empty());
}

#[test]
fn one_byte_reader_chunks_produce_the_same_module_as_text() {
    let text = "// 幅\r\n ";
    let reader = BufReader::with_capacity(1, text.as_bytes());

    assert_eq!(parse(reader).unwrap(), parse_from_str(text).unwrap());
}

#[test]
fn comment_and_trailing_whitespace_cover_the_whole_module() {
    let text = "// 幅\r\n ";
    let module = parse_from_str(text).unwrap();
    let source = module.source.unwrap();
    assert_eq!(source.start, Cursor::default());
    assert_eq!(
        (source.end.offset, source.end.line, source.end.column),
        (text.len(), 2, 2)
    );
    let [comment, space] = source.trivia.as_slice() else {
        panic!("expected a comment followed by whitespace");
    };
    assert_eq!(comment.kind, TriviaKind::LineComment(" 幅".into()));
    assert_eq!(space.kind, TriviaKind::Whitespace("\r\n ".into()));
    assert_eq!(comment.start.offset..comment.end.offset, 0.."// 幅".len());
    assert_eq!(comment.start, source.start);
    assert_eq!(space.start, comment.end);
    assert_eq!(space.end, source.end);
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
    let ParseError::Lexical(LexError::Input(error)) = error else {
        panic!("expected an input error");
    };
    assert_eq!(error.cause.kind(), ErrorKind::InvalidData);
    assert_eq!(error.start.offset, 1);
}
