use super::super::name::{identifier, qualified_name};
use super::{node_bytes, node_source, read_complete};
use crate::{
    ito::{ParseError, lexer},
    syntax::ast::TriviaKind,
};

#[test]
fn identifier_starts_at_the_current_token() {
    for source in [" width", "// name\nwidth"] {
        let error = read_complete(lexer::from_str(source), identifier()).unwrap_err();
        assert!(matches!(error, ParseError::Unexpected { expected, cursor }
            if expected == "an identifier" && cursor.offset == 0));
    }
}

#[test]
fn identifier_parsing_records_its_range_and_requires_complete_input() {
    let node = read_complete(lexer::from_str("width"), identifier()).unwrap();
    assert_eq!(node.value.as_str(), "width");
    assert_eq!(node_bytes(&node), 0..5);
    assert!(node_source(&node).trivia.is_empty());
    let error = read_complete(lexer::from_str("width// name"), identifier()).unwrap_err();
    assert!(matches!(error, ParseError::Unexpected { expected, cursor }
        if expected == "end of input" && cursor.offset == 5));
    assert!(matches!(
        read_complete(lexer::from_str("panel.width"), identifier()),
        Err(ParseError::Unexpected { expected, cursor })
            if expected == "end of input" && cursor.offset == 5
    ));
}

#[test]
fn qualified_names_own_their_parts_and_locations_independently_of_source() {
    let name = {
        let source = String::from("panels.Panel// name");
        read_complete(lexer::from_str(&source), qualified_name()).unwrap()
    };
    let parts = name.value.as_slice();
    let names: Vec<_> = parts.iter().map(|node| node.value.as_str()).collect();
    assert_eq!(names, ["panels", "Panel"]);
    assert_eq!(node_bytes(&name), 0..12);
    let ranges: Vec<_> = parts.iter().map(node_bytes).collect();
    assert_eq!(ranges, [0..6, 7..12]);
    assert_eq!(
        node_source(&name).trivia[0].kind,
        TriviaKind::LineComment(" name".into())
    );
}

#[test]
fn malformed_names_report_the_failing_part_instead_of_a_later_token() {
    for (source, offset, expected) in [
        ("(", 0, "an identifier"),
        ("\"Panel\"", 0, "an identifier"),
        ("2width;", 0, "an identifier"),
        ("panel-width", 0, "an identifier"),
        ("幅", 0, "an identifier"),
        ("panels.", 7, "an identifier"),
        ("panels..Panel", 7, "an identifier"),
        ("panels.2width;", 7, "an identifier"),
        ("panels.Panel<T>", 12, "end of input"),
        ("panels Panel", 7, "end of input"),
    ] {
        let error = read_complete(lexer::from_str(source), qualified_name()).unwrap_err();
        let ParseError::Unexpected {
            expected: actual,
            cursor,
            ..
        } = error
        else {
            panic!("expected a syntax error for {source:?}");
        };
        assert_eq!(cursor.offset, offset, "{source:?}");
        assert_eq!(actual, expected, "{source:?}");
    }
}

#[test]
fn names_from_streams_keep_comments_and_positions_across_lines() {
    let source = "panels // 幅\r\n . Panel // end";
    let borrowed = read_complete(lexer::from_str(source), qualified_name()).unwrap();
    let streamed = read_complete(
        lexer::from_reader(std::io::BufReader::with_capacity(1, source.as_bytes())),
        qualified_name(),
    )
    .unwrap();
    assert_eq!(borrowed, streamed);
    let name = borrowed;
    let location = node_source(&name);
    assert_eq!((location.start.line, location.start.column), (1, 1));
    assert_eq!((location.end.line, location.end.column), (2, 9));
    assert_eq!(
        &source[location.start.offset..location.end.offset],
        "panels // 幅\r\n . Panel"
    );
    let parts = name.value.as_slice();
    assert_eq!(parts.len(), 2);
    assert_eq!(&source[node_bytes(&parts[0])], "panels");
    assert_eq!(&source[node_bytes(&parts[1])], "Panel");
    let comments: Vec<_> = location
        .trivia
        .iter()
        .filter_map(|item| match &item.kind {
            TriviaKind::LineComment(text) => Some(text.as_ref()),
            _ => None,
        })
        .collect();
    assert_eq!(comments, [" 幅", " end"]);
}

#[test]
fn missing_parts_at_eoi_use_the_position_after_trivia() {
    let source = "panels. // missing\r\n";
    let error = read_complete(lexer::from_str(source), qualified_name()).unwrap_err();
    let ParseError::Unexpected { expected, cursor } = error else {
        panic!("expected an identifier error at end of input");
    };
    assert_eq!(cursor.offset, source.len());
    assert_eq!((cursor.line, cursor.column), (2, 1));
    assert_eq!(expected, "an identifier");
}

#[test]
fn lexical_failures_propagate_from_name_parts_and_trailing_input() {
    for source in ["panels.\"unfinished", "panels.Panel \"unfinished"] {
        let error = read_complete(lexer::from_str(source), qualified_name()).unwrap_err();
        assert!(matches!(error, ParseError::Lexical(_)), "{source:?}");
    }
    let error =
        read_complete(lexer::from_reader(&b"panels.\xff"[..]), qualified_name()).unwrap_err();
    assert!(matches!(error, ParseError::Lexical(_)));
}
