use aition_core::{
    lexer,
    syntax::{
        TokenStream,
        ast::{AstNode, Identifier, QualifiedName, Span, Type},
        parse,
    },
};

fn span<T>(node: &AstNode<T>) -> &Span {
    node.span.as_ref().expect("parsed node must have a span")
}

fn ranges<T>(nodes: &[AstNode<T>]) -> Vec<std::ops::Range<usize>> {
    nodes.iter().map(|node| span(node).range.clone()).collect()
}

#[test]
fn each_array_suffix_has_its_own_parent_range() {
    let parsed = parse::<AstNode<Type>>(TokenStream::new(lexer::from_str("A**"))).unwrap();
    let outer = parsed.syntax();
    assert_eq!(span(outer).range, 0..3);
    let Type::Array(inner) = &outer.value else {
        panic!("expected outer array")
    };
    assert_eq!(span(inner).range, 0..2);
    let Type::Array(named) = &inner.value else {
        panic!("expected inner array")
    };
    assert_eq!(span(named).range, 0..1);
    let Type::Named { name, .. } = &named.value else {
        panic!("expected named type")
    };
    assert_eq!(span(name).range, 0..1);
    let parts = name.value.as_slice();
    assert_eq!(parts.len(), 1);
    assert_eq!(span(&parts[0]).range, 0..1);
    assert_eq!(parts[0].value.as_str(), "A");
}

#[test]
fn grouping_and_union_nodes_keep_their_own_spans() {
    let parsed =
        parse::<AstNode<Type>>(TokenStream::new(lexer::from_str(" (A | B*) // tail"))).unwrap();
    let group = parsed.syntax();
    assert_eq!(span(group).range, 1..9);
    let Type::Group(union) = &group.value else {
        panic!("expected group")
    };
    assert_eq!(span(union).range, 2..8);
    let Type::Union(items) = &union.value else {
        panic!("expected union")
    };
    assert_eq!(ranges(items), [2..3, 6..8]);
    let Type::Array(inner) = &items[1].value else {
        panic!("expected array")
    };
    assert_eq!(span(inner).range, 6..7);
    let trailing_comment = parsed.trivia().last().unwrap();
    assert_eq!(trailing_comment.span, 10..17);
    assert!(trailing_comment.span.start >= span(group).range.end);

    let flat = parse::<AstNode<Type>>(TokenStream::new(lexer::from_str("A | B | C"))).unwrap();
    let Type::Union(items) = &flat.syntax().value else {
        panic!("expected flat union")
    };
    assert_eq!(ranges(items), [0..1, 4..5, 8..9]);
}

#[test]
fn named_and_function_types_carry_spans_in_their_fields() {
    let named = parse::<AstNode<Type>>(TokenStream::new(lexer::from_str("p.Box<A, B>"))).unwrap();
    assert_eq!(span(named.syntax()).range, 0..11);
    let Type::Named { name, arguments } = &named.syntax().value else {
        panic!("expected named type")
    };
    assert_eq!(span(name).range, 0..5);
    assert_eq!(ranges(name.value.as_slice()), [0..1, 2..5]);
    assert_eq!(ranges(arguments), [6..7, 9..10]);

    let function = parse::<AstNode<Type>>(TokenStream::new(lexer::from_str("[A, B] (C)"))).unwrap();
    assert_eq!(span(function.syntax()).range, 0..10);
    let Type::Function { parameters, result } = &function.syntax().value else {
        panic!("expected function")
    };
    assert_eq!(ranges(parameters), [1..2, 4..5]);
    assert_eq!(span(result).range, 8..9);
    let empty = parse::<AstNode<Type>>(TokenStream::new(lexer::from_str("[] (C)"))).unwrap();
    let Type::Function { parameters, result } = &empty.syntax().value else {
        panic!("expected function")
    };
    assert!(parameters.is_empty());
    assert_eq!(span(result).range, 4..5);
}

#[test]
fn spans_survive_source_release_and_exclude_lookahead_trivia() {
    let parsed = {
        let source = String::from("\t//幅😀\r\n  A* //尾");
        parse::<AstNode<Type>>(TokenStream::new(lexer::from_str(&source))).unwrap()
    };
    let location = span(parsed.syntax());
    assert_eq!(location.range, 14..16);
    assert_eq!((location.start.line, location.start.column), (2, 3));
    let Type::Array(inner) = &parsed.syntax().value else {
        panic!("expected array")
    };
    assert_eq!(span(inner).range, 14..15);
    assert_eq!(parsed.trivia().last().unwrap().span, 17..22);

    let source = "\t//幅😀\r\n  A* //尾";
    for capacity in 1..=8 {
        let reader = std::io::BufReader::with_capacity(capacity, source.as_bytes());
        let streamed =
            parse::<AstNode<Type>>(TokenStream::new(lexer::from_reader(reader))).unwrap();
        assert_eq!(streamed, parsed);
    }
}

#[test]
fn syntax_can_be_constructed_without_a_source_and_combined_with_parsed_nodes() {
    let name = QualifiedName::new(Identifier::new("A").unwrap().into(), []);
    let synthetic = AstNode::new(Type::Named {
        name: name.into(),
        arguments: vec![],
    });
    assert!(synthetic.span.is_none());
    let Type::Named { name, .. } = &synthetic.value else {
        panic!("expected named type")
    };
    assert!(name.span.is_none());
    assert!(name.value.as_slice()[0].span.is_none());

    let parsed = parse::<AstNode<Type>>(TokenStream::new(lexer::from_str("B*")))
        .unwrap()
        .into_syntax();
    let union = AstNode::new(Type::Union(vec![synthetic, parsed]));
    assert!(union.span.is_none());
    let Type::Union(items) = union.value else {
        panic!("expected union")
    };
    assert!(items[0].span.is_none());
    assert_eq!(span(&items[1]).range, 0..2);
    let Type::Array(inner) = &items[1].value else {
        panic!("expected array")
    };
    assert_eq!(span(inner).range, 0..1);
}
