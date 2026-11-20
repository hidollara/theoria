use aition_core::{
    input::Cursor,
    ito::{LexError, ParseError, parse, parse_from_str},
    syntax::ast::{ModuleItem, TriviaKind},
};
use std::io::{BufReader, ErrorKind};

#[test]
fn empty_module_has_an_empty_span() {
    let module = parse_from_str("").unwrap();
    assert!(module.value.items.is_empty());
    assert_eq!(parse(&b""[..]).unwrap(), module);
    let source = module.source.unwrap();
    assert_eq!(source.start, Cursor::default());
    assert_eq!(source.end, source.start);
    assert!(source.trivia.is_empty());
}

#[test]
fn type_declarations_keep_their_names_and_source_order() {
    let text = "// file\ntype// name\r\nCanvas// signature\n:{} type _Width2:{}";
    let module = parse_from_str(text).unwrap();
    let reader = BufReader::with_capacity(1, text.as_bytes());
    assert_eq!(parse(reader).unwrap(), module);

    let [first, second] = module.value.items.as_slice() else {
        panic!("expected two type declarations");
    };
    let ModuleItem::Type {
        name,
        constructor,
        members,
    } = &first.value;
    assert!(constructor.is_none());
    assert!(members.value.is_empty());
    assert_eq!(name.value.as_str(), "Canvas");
    let name_source = name.source.as_ref().unwrap();
    assert_eq!(
        &text[name_source.start.offset..name_source.end.offset],
        "Canvas"
    );
    assert!(name_source.trivia.is_empty());
    let declaration = first.source.as_ref().unwrap();
    assert_eq!(
        &text[declaration.start.offset..declaration.end.offset],
        "type// name\r\nCanvas// signature\n:{}"
    );
    let [comment, newline, signature, line_break] = declaration.trivia.as_slice() else {
        panic!("expected trivia before the name and before the colon");
    };
    assert_eq!(comment.kind, TriviaKind::LineComment(" name".into()));
    assert_eq!(newline.kind, TriviaKind::Whitespace("\r\n".into()));
    assert_eq!(signature.kind, TriviaKind::LineComment(" signature".into()));
    assert_eq!(line_break.kind, TriviaKind::Whitespace("\n".into()));

    let ModuleItem::Type { name, .. } = &second.value;
    assert_eq!(name.value.as_str(), "_Width2");
    let root = module.source.unwrap();
    let [comment, newline, between] = root.trivia.as_slice() else {
        panic!("expected only trivia outside the declarations");
    };
    assert_eq!(comment.kind, TriviaKind::LineComment(" file".into()));
    assert_eq!(newline.end, declaration.start);
    assert_eq!(between.start, declaration.end);
    assert_eq!(between.kind, TriviaKind::Whitespace(" ".into()));
}

#[test]
fn constructor_and_member_lists_keep_their_order_and_source() {
    let declaration = "type Panel: [Width, Height,] // members\n{ width: Width, height: Height, }";
    let text = format!("{declaration} type Br: [] {{}} // next\ntype Size: {{}}");
    let module = parse_from_str(&text).unwrap();
    assert_eq!(
        parse(BufReader::with_capacity(1, text.as_bytes())).unwrap(),
        module
    );
    let [panel, br, size] = module.value.items.as_slice() else {
        panic!("expected three type declarations");
    };
    let ModuleItem::Type {
        constructor,
        members,
        ..
    } = &panel.value;
    let inputs = &constructor.as_ref().unwrap().value;
    assert_eq!(
        inputs
            .iter()
            .map(|ty| ty.value.as_str())
            .collect::<Vec<_>>(),
        ["Width", "Height"]
    );
    let members = &members.value;
    assert_eq!(
        members
            .iter()
            .map(|m| (m.value.name.value.as_str(), m.value.ty.value.as_str()))
            .collect::<Vec<_>>(),
        [("width", "Width"), ("height", "Height")]
    );
    let source = panel.source.as_ref().unwrap();
    assert_eq!(&text[source.start.offset..source.end.offset], declaration);
    assert!(
        source
            .trivia
            .iter()
            .any(|t| t.kind == TriviaKind::LineComment(" members".into()))
    );
    let ModuleItem::Type {
        constructor,
        members,
        ..
    } = &br.value;
    assert!(constructor.as_ref().unwrap().value.is_empty());
    assert!(members.value.is_empty());
    let ModuleItem::Type {
        constructor,
        members,
        ..
    } = &size.value;
    assert!(constructor.is_none());
    assert!(members.value.is_empty());
    assert!(
        module
            .source
            .unwrap()
            .trivia
            .iter()
            .any(|t| t.kind == TriviaKind::LineComment(" next".into()))
    );
}

#[test]
fn malformed_signatures_report_the_missing_part() {
    for (text, expected, offset) in [
        ("type Br", ":", 7),
        ("type Br:", "[ or {", 8),
        ("type Br: []", "{", 11),
        ("type Br: [] type Other: {}", "{", 12),
        ("type Br: [,]", "an identifier", 10),
        ("type Br: [A B]", ", or ]", 12),
        ("type Br: [A", ", or ]", 11),
        ("type Br: { x }", ":", 13),
        ("type Br: { x: }", "an identifier", 14),
        ("type Br: { x: A", ", or }", 15),
    ] {
        let error = parse_from_str(text).unwrap_err();
        assert!(
            matches!(error, ParseError::Unexpected { expected: ref actual, cursor }
            if actual == expected && cursor.offset == offset),
            "{text:?}: {error}"
        );
    }
}

#[test]
fn malformed_type_declarations_report_the_expected_token_at_its_position() {
    for (text, expected, offset) in [
        ("typeName", "\"type\"", 0),
        ("type A;", "an identifier", 5),
        ("type ", "an identifier", 5),
        ("type 2D", "an identifier", 5),
        ("type A-B", "an identifier", 5),
        ("type A:{} type", "an identifier", 14),
    ] {
        let error = parse_from_str(text).unwrap_err();
        assert!(
            matches!(error, ParseError::Unexpected { expected: ref actual, cursor }
            if actual == expected && cursor.offset == offset),
            "{text:?}: {error}"
        );
    }
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
