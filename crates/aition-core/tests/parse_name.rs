use aition_core::{
    lexer,
    syntax::{
        ParseError, TokenStream, TriviaKind,
        ast::{AstNode, Identifier, QualifiedName},
        parse,
    },
};

#[test]
fn ast_node_uses_the_same_parser_and_adds_the_consumed_span() {
    let source = " // name\n width ";
    let (plain, plain_trivia) =
        parse::<Identifier>(TokenStream::new(lexer::from_str(source))).unwrap();
    let (located, trivia) =
        parse::<AstNode<Identifier>>(TokenStream::new(lexer::from_str(source))).unwrap();
    assert_eq!(plain.as_str(), "width");
    assert_eq!(located.value, plain);
    let span = located.span.unwrap();
    assert_eq!(span.range(), 10..15);
    assert_eq!((span.start.line, span.start.column), (2, 2));
    assert_eq!((span.end.line, span.end.column), (2, 7));
    assert_eq!(trivia, plain_trivia);
}

#[test]
fn identifier_parsing_records_its_range_and_requires_complete_input() {
    let (identifier, trivia) =
        parse::<AstNode<Identifier>>(TokenStream::new(lexer::from_str("width// name"))).unwrap();
    assert_eq!(identifier.value.as_str(), "width");
    assert_eq!(identifier.span.unwrap().range(), 0..5);
    assert_eq!(trivia[0].kind, TriviaKind::LineComment(" name".into()));
    assert!(matches!(
        parse::<Identifier>(TokenStream::new(lexer::from_str("panel.width"))),
        Err(ParseError::Unexpected { expected, cursor })
            if expected == "end of input" && cursor.offset == 5
    ));
}

#[test]
fn qualified_names_own_their_parts_and_locations_independently_of_source() {
    let (name, trivia) = {
        let source = String::from("panels.Panel// name");
        parse::<AstNode<QualifiedName>>(TokenStream::new(lexer::from_str(&source))).unwrap()
    };
    let parts = name.value.as_slice();
    let names: Vec<_> = parts.iter().map(|node| node.value.as_str()).collect();
    assert_eq!(names, ["panels", "Panel"]);
    assert_eq!(name.span.as_ref().unwrap().range(), 0..12);
    let ranges: Vec<_> = parts
        .iter()
        .map(|node| node.span.as_ref().unwrap().range())
        .collect();
    assert_eq!(ranges, [0..6, 7..12]);
    assert_eq!(trivia[0].kind, TriviaKind::LineComment(" name".into()));
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
        let error = parse::<QualifiedName>(TokenStream::new(lexer::from_str(source))).unwrap_err();
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
    let source = " // 幅\r\n panels // between\r\n . Panel // end";
    let borrowed =
        parse::<AstNode<QualifiedName>>(TokenStream::new(lexer::from_str(source))).unwrap();
    let streamed = parse::<AstNode<QualifiedName>>(TokenStream::new(lexer::from_reader(
        std::io::BufReader::with_capacity(1, source.as_bytes()),
    )))
    .unwrap();
    assert_eq!(borrowed, streamed);
    let (name, trivia) = borrowed;
    let span = name.span.unwrap();
    assert_eq!((span.start.line, span.start.column), (2, 2));
    assert_eq!((span.end.line, span.end.column), (3, 9));
    assert_eq!(&source[span.range()], "panels // between\r\n . Panel");
    let parts = name.value.as_slice();
    assert_eq!(parts.len(), 2);
    assert_eq!(&source[parts[0].span.as_ref().unwrap().range()], "panels");
    assert_eq!(&source[parts[1].span.as_ref().unwrap().range()], "Panel");
    let comments: Vec<_> = trivia
        .iter()
        .filter_map(|item| match &item.kind {
            TriviaKind::LineComment(text) => Some(text.as_ref()),
            _ => None,
        })
        .collect();
    assert_eq!(comments, [" 幅", " between", " end"]);
}

#[test]
fn missing_parts_at_eof_use_the_position_after_trivia() {
    let source = "panels. // missing\r\n";
    let error = parse::<QualifiedName>(TokenStream::new(lexer::from_str(source))).unwrap_err();
    let ParseError::Unexpected { expected, cursor } = error else {
        panic!("expected an identifier error at EOF");
    };
    assert_eq!(cursor.offset, source.len());
    assert_eq!((cursor.line, cursor.column), (2, 1));
    assert_eq!(expected, "an identifier");
}

#[test]
fn lexical_failures_propagate_from_name_parts_and_trailing_input() {
    for source in ["panels.\"unfinished", "panels.Panel \"unfinished"] {
        let error = parse::<QualifiedName>(TokenStream::new(lexer::from_str(source))).unwrap_err();
        assert!(matches!(error, ParseError::Lexical(_)), "{source:?}");
    }
    let error = parse::<QualifiedName>(TokenStream::new(lexer::from_reader(&b"panels.\xff"[..])))
        .unwrap_err();
    assert!(matches!(error, ParseError::Lexical(_)));
}
