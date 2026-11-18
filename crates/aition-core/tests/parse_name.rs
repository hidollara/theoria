use aition_core::{
    lexer,
    syntax::{
        Expected, ParseError, TokenStream, TriviaKind,
        ast::{AstNode, Identifier, QualifiedName},
        parse,
    },
};

#[test]
fn ast_node_uses_the_same_parser_and_adds_the_consumed_span() {
    let source = " // name\n width ";
    let plain = parse::<Identifier>(TokenStream::new(lexer::from_str(source))).unwrap();
    let located = parse::<AstNode<Identifier>>(TokenStream::new(lexer::from_str(source))).unwrap();
    assert_eq!(plain.syntax().as_str(), "width");
    assert_eq!(&located.syntax().value, plain.syntax());
    assert_eq!(located.syntax().span.as_ref().unwrap().range, 10..15);
    assert_eq!(located.trivia(), plain.trivia());
}

#[test]
fn identifier_parsing_records_its_range_and_requires_complete_input() {
    let parsed =
        parse::<AstNode<Identifier>>(TokenStream::new(lexer::from_str("width// name"))).unwrap();
    assert_eq!(parsed.syntax().value.as_str(), "width");
    assert_eq!(parsed.syntax().span.as_ref().unwrap().range, 0..5);
    assert_eq!(
        parsed.trivia()[0].kind,
        TriviaKind::LineComment(" name".into())
    );
    assert!(matches!(
        parse::<AstNode<Identifier>>(TokenStream::new(lexer::from_str("panel.width"))),
        Err(ParseError::Unexpected {
            expected: Expected::EndOfInput,
            ..
        })
    ));
}

#[test]
fn qualified_names_own_their_parts_and_locations_independently_of_source() {
    let parsed = {
        let source = String::from("panels.Panel// name");
        parse::<AstNode<QualifiedName>>(TokenStream::new(lexer::from_str(&source))).unwrap()
    };
    let names: Vec<_> = parsed
        .syntax()
        .value
        .as_slice()
        .iter()
        .map(|node| node.value.as_str())
        .collect();
    assert_eq!(names, ["panels", "Panel"]);
    assert_eq!(parsed.syntax().span.as_ref().unwrap().range, 0..12);
    let ranges: Vec<_> = parsed
        .syntax()
        .value
        .as_slice()
        .iter()
        .map(|node| node.span.as_ref().unwrap().range.clone())
        .collect();
    assert_eq!(ranges, [0..6, 7..12]);
    assert_eq!(
        parsed.trivia()[0].kind,
        TriviaKind::LineComment(" name".into())
    );
    for (source, expected) in [
        ("panels.", Expected::Identifier),
        ("panels.Panel<T>", Expected::EndOfInput),
    ] {
        let ParseError::Unexpected {
            expected: actual, ..
        } = parse::<AstNode<QualifiedName>>(TokenStream::new(lexer::from_str(source))).unwrap_err()
        else {
            panic!("expected a syntax error");
        };
        assert_eq!(actual, expected);
    }
}
