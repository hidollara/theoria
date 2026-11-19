use super::super::types::type_expr;
use super::read_complete;
use crate::{
    ito::lexer::{self, TokenKind, TokenStream, TokenStreamItem},
    syntax::ast::{AstNode, QualifiedName, Trivia, TriviaKind, Type},
};

fn comments(trivia: &[Trivia]) -> Vec<&str> {
    trivia
        .iter()
        .filter_map(|item| match &item.kind {
            TriviaKind::LineComment(text) => Some(text.as_ref()),
            _ => None,
        })
        .collect()
}

fn name_trivia<'a>(node: &'a AstNode<QualifiedName>, all: &mut Vec<&'a Trivia>) {
    all.extend(&node.source.as_ref().unwrap().trivia);
    for part in node.value.as_slice() {
        all.extend(&part.source.as_ref().unwrap().trivia);
    }
}

fn type_trivia<'a>(node: &'a AstNode<Type>, all: &mut Vec<&'a Trivia>) {
    all.extend(&node.source.as_ref().unwrap().trivia);
    match &node.value {
        Type::Named { name, arguments } => {
            name_trivia(name, all);
            for argument in arguments {
                type_trivia(argument, all);
            }
        }
        Type::Array(inner) | Type::Group(inner) => type_trivia(inner, all),
        Type::Union(items) => {
            for item in items {
                type_trivia(item, all);
            }
        }
        Type::Function { parameters, result } => {
            for parameter in parameters {
                type_trivia(parameter, all);
            }
            type_trivia(result, all);
        }
    }
}

fn assert_exactly_once(source: &str, mut actual: Vec<&Trivia>) {
    actual.sort_by_key(|item| item.start.offset);
    let mut stream = lexer::from_str(source);
    let mut expected = Vec::new();
    while let TokenStreamItem::Token(token) = stream.next().unwrap() {
        let kind = match token.kind {
            TokenKind::Whitespace(text) => TriviaKind::Whitespace(text.into()),
            TokenKind::LineComment(text) => TriviaKind::LineComment(text.into()),
            _ => continue,
        };
        expected.push(Trivia {
            kind,
            start: token.start,
            end: token.end,
        });
    }
    assert_eq!(actual, expected.iter().collect::<Vec<_>>());
}

#[test]
fn nested_types_own_their_trivia_without_copying_it_to_parents() {
    let source = "* // array\n(T< A // generic\n > | B) // end";
    let root = read_complete(lexer::from_str(source), type_expr()).unwrap();
    let Type::Array(group) = &root.value else {
        panic!("expected array")
    };
    let Type::Group(union) = &group.value else {
        panic!("expected group")
    };
    let Type::Union(items) = &union.value else {
        panic!("expected union")
    };
    assert_eq!(
        comments(&root.source.as_ref().unwrap().trivia),
        [" array", " end"]
    );
    assert!(comments(&group.source.as_ref().unwrap().trivia).is_empty());
    assert!(comments(&union.source.as_ref().unwrap().trivia).is_empty());
    let Type::Named { arguments, .. } = &items[0].value else {
        panic!("expected generic type")
    };
    let Type::Named { name, .. } = &arguments[0].value else {
        panic!("expected named argument")
    };
    assert_eq!(
        comments(&name.source.as_ref().unwrap().trivia),
        [" generic"]
    );
    let mut all = vec![];
    type_trivia(&root, &mut all);
    assert_exactly_once(source, all);
}

#[test]
fn single_type_terms_keep_trivia_when_temporary_nodes_are_unwrapped() {
    for source in [
        "( // before\nT< A // inner\n >) // after",
        "[A // input\n]( * B )",
    ] {
        let node = read_complete(lexer::from_str(source), type_expr()).unwrap();
        let mut all = vec![];
        type_trivia(&node, &mut all);
        assert_exactly_once(source, all);
    }
}
