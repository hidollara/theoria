use super::super::types::type_expr;
use super::{node_bytes, node_source, read_complete};
use crate::{
    ito::lexer,
    syntax::ast::{AstNode, Identifier, QualifiedName, Type},
};

fn ranges<T>(nodes: &[AstNode<T>]) -> Vec<std::ops::Range<usize>> {
    nodes.iter().map(node_bytes).collect()
}

#[test]
fn each_array_prefix_has_its_own_parent_range() {
    let parsed = read_complete(lexer::from_str("**A"), type_expr()).unwrap();
    let outer = &parsed;
    assert_eq!(node_bytes(outer), 0..3);
    let Type::Array(inner) = &outer.value else {
        panic!("expected outer array")
    };
    assert_eq!(node_bytes(inner), 1..3);
    let Type::Array(named) = &inner.value else {
        panic!("expected inner array")
    };
    assert_eq!(node_bytes(named), 2..3);
    let Type::Named { name, .. } = &named.value else {
        panic!("expected named type")
    };
    assert_eq!(node_bytes(name), 2..3);
    let parts = name.value.as_slice();
    assert_eq!(parts.len(), 1);
    assert_eq!(node_bytes(&parts[0]), 2..3);
    assert_eq!(parts[0].value.as_str(), "A");
}

#[test]
fn grouping_and_union_nodes_keep_their_own_spans() {
    let parsed = read_complete(lexer::from_str("( A | *B) // tail"), type_expr()).unwrap();
    let group = &parsed;
    assert_eq!(node_bytes(group), 0..9);
    let Type::Group(union) = &group.value else {
        panic!("expected group")
    };
    assert_eq!(node_bytes(union), 2..8);
    let Type::Union(items) = &union.value else {
        panic!("expected union")
    };
    assert_eq!(ranges(items), [2..3, 6..8]);
    let Type::Array(inner) = &items[1].value else {
        panic!("expected array")
    };
    assert_eq!(node_bytes(inner), 7..8);
    let trailing_comment = node_source(&parsed).trivia.last().unwrap();
    assert_eq!(
        trailing_comment.start.offset..trailing_comment.end.offset,
        10..17
    );
    assert!(trailing_comment.start.offset >= node_source(group).end.offset);

    let flat = read_complete(lexer::from_str("A | B | C"), type_expr()).unwrap();
    let Type::Union(items) = &flat.value else {
        panic!("expected flat union")
    };
    assert_eq!(ranges(items), [0..1, 4..5, 8..9]);
}

#[test]
fn named_and_function_types_carry_spans_in_their_fields() {
    let named = read_complete(lexer::from_str("p.Box<A, B>"), type_expr()).unwrap();
    assert_eq!(node_bytes(&named), 0..11);
    let Type::Named { name, arguments } = &named.value else {
        panic!("expected named type")
    };
    assert_eq!(node_bytes(name), 0..5);
    assert_eq!(ranges(name.value.as_slice()), [0..1, 2..5]);
    assert_eq!(ranges(arguments), [6..7, 9..10]);

    let function = read_complete(lexer::from_str("[A, B] (C)"), type_expr()).unwrap();
    assert_eq!(node_bytes(&function), 0..10);
    let Type::Function { parameters, result } = &function.value else {
        panic!("expected function")
    };
    assert_eq!(ranges(parameters), [1..2, 4..5]);
    assert_eq!(node_bytes(result), 8..9);
    let empty = read_complete(lexer::from_str("[] (C)"), type_expr()).unwrap();
    let Type::Function { parameters, result } = &empty.value else {
        panic!("expected function")
    };
    assert!(parameters.is_empty());
    assert_eq!(node_bytes(result), 4..5);
}

#[test]
fn spans_survive_source_release_and_exclude_lookahead_trivia() {
    let parsed = {
        let source = String::from("*\t//幅😀\r\n  A //尾");
        read_complete(lexer::from_str(&source), type_expr()).unwrap()
    };
    let location = node_source(&parsed);
    assert_eq!(location.start.offset..location.end.offset, 0..16);
    assert_eq!((location.start.line, location.start.column), (1, 1));
    let Type::Array(inner) = &parsed.value else {
        panic!("expected array")
    };
    assert_eq!(node_bytes(inner), 15..16);
    let Type::Named { name, .. } = &inner.value else {
        panic!("expected named type")
    };
    let comment = node_source(name).trivia.last().unwrap();
    assert_eq!(comment.start.offset..comment.end.offset, 17..22);

    let source = "*\t//幅😀\r\n  A //尾";
    for capacity in 1..=8 {
        let reader = std::io::BufReader::with_capacity(capacity, source.as_bytes());
        let streamed = read_complete(lexer::from_reader(reader), type_expr()).unwrap();
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
    assert!(synthetic.source.is_none());
    let Type::Named { name, .. } = &synthetic.value else {
        panic!("expected named type")
    };
    assert!(name.source.is_none());
    assert!(name.value.as_slice()[0].source.is_none());

    let parsed = read_complete(lexer::from_str("*B"), type_expr()).unwrap();
    let union = AstNode::new(Type::Union(vec![synthetic, parsed]));
    assert!(union.source.is_none());
    let Type::Union(items) = union.value else {
        panic!("expected union")
    };
    assert!(items[0].source.is_none());
    assert_eq!(node_bytes(&items[1]), 0..2);
    let Type::Array(inner) = &items[1].value else {
        panic!("expected array")
    };
    assert_eq!(node_bytes(inner), 1..2);
}

#[test]
fn array_prefixes_include_internal_trivia_but_children_start_at_their_own_token() {
    let source = "* // outer\n * A // tail";
    let outer = read_complete(lexer::from_str(source), type_expr()).unwrap();
    assert_eq!(&source[node_bytes(&outer)], "* // outer\n * A");
    let Type::Array(inner) = &outer.value else {
        panic!("expected outer array")
    };
    assert_eq!(&source[node_bytes(inner)], "* A");
    assert_eq!(
        (
            node_source(inner).start.line,
            node_source(inner).start.column
        ),
        (2, 2)
    );
    let Type::Array(element) = &inner.value else {
        panic!("expected inner array")
    };
    assert_eq!(&source[node_bytes(element)], "A");
}
