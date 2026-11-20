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
fn module_owns_file_trivia_across_reader_boundaries() {
    let text = "// 幅\r\n ";
    let module = parse_from_str(text).unwrap();
    let reader = BufReader::with_capacity(1, text.as_bytes());
    assert_eq!(parse(reader).unwrap(), module);
    let source = module.source.unwrap();
    assert_eq!(source.start, source.end);
    assert_eq!(
        (source.end.offset, source.end.line, source.end.column),
        (text.len(), 2, 2)
    );
    let [comment, space] = source.trivia.as_slice() else {
        panic!("expected a comment followed by whitespace");
    };
    assert_eq!(comment.kind, TriviaKind::LineComment(" 幅".into()));
    assert_eq!(space.kind, TriviaKind::Whitespace("\r\n ".into()));
}

#[test]
fn both_inputs_report_the_first_unexpected_token() {
    let text = "// 幅\r\n )";
    for error in [
        parse_from_str(text).unwrap_err(),
        parse(text.as_bytes()).unwrap_err(),
    ] {
        let ParseError::Unexpected { cursor, .. } = error else {
            panic!("expected a syntax error");
        };
        assert_eq!(cursor.offset, text.find(')').unwrap());
    }
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
