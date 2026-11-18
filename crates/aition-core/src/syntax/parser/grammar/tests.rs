use super::{Parser, symbol};
use crate::lexer::{self, Symbol};
use crate::syntax::parser::{Expected, ParseError, node::AstNodeParser, token::TokenStream};

#[test]
fn choice_tries_the_other_rule_only_before_consumption() {
    let grammar = symbol(Symbol::OpenParen)
        .then(symbol(Symbol::CloseParen))
        .or(symbol(Symbol::OpenBracket).then(symbol(Symbol::CloseBracket)));
    let (value, tokens) = grammar
        .read(TokenStream::new(lexer::from_str("[]")))
        .unwrap();
    assert_eq!(value, Some(((), ())));
    tokens.finish().unwrap();
    let error = grammar
        .read(TokenStream::new(lexer::from_str("(]")))
        .err()
        .unwrap();
    assert!(matches!(
        error,
        ParseError::Unexpected {
            expected: Expected::Symbol(Symbol::CloseParen),
            ..
        }
    ));
}

#[test]
fn optional_does_not_hide_an_incomplete_construct() {
    let grammar = symbol(Symbol::OpenAngle)
        .then(symbol(Symbol::CloseAngle))
        .optional();
    let (value, tokens) = grammar
        .read(TokenStream::new(lexer::from_str(" // empty")))
        .unwrap();
    assert_eq!(value, Some(None));
    assert_eq!(tokens.finish().unwrap().len(), 2);
    assert!(
        grammar
            .read(TokenStream::new(lexer::from_str("<")))
            .is_err()
    );
}

#[test]
fn repetition_keeps_trivia_once_even_when_it_peeks_past_the_last_match() {
    let grammar = symbol(Symbol::Asterisk).repeated();
    let (values, tokens) = grammar
        .read(TokenStream::new(lexer::from_str("* // between\n* // end")))
        .unwrap();
    assert_eq!(values.unwrap().len(), 2);
    let trivia = tokens.finish().unwrap();
    assert_eq!(
        trivia
            .iter()
            .filter(|item| matches!(item.kind, crate::syntax::parser::TriviaKind::LineComment(_)))
            .count(),
        2
    );
}

#[test]
#[should_panic(expected = "a repeated grammar must consume a token")]
fn repetition_rejects_empty_success_instead_of_looping_forever() {
    let _ = symbol(Symbol::Asterisk)
        .optional()
        .repeated()
        .read(TokenStream::new(lexer::from_str("")));
}

#[test]
fn an_unconsumed_alternative_discards_its_constructed_node() {
    let empty_node = AstNodeParser(symbol(Symbol::Asterisk).optional().map(|_| ()));
    let grammar = empty_node
        .then(symbol(Symbol::Comma))
        .map(|(node, _)| node)
        .or(AstNodeParser(symbol(Symbol::Semicolon).map(|_| ())));
    let (node, tokens) = grammar
        .read(TokenStream::new(lexer::from_str(";")))
        .unwrap();
    assert_eq!(node.unwrap().span.unwrap().range, 0..1);
    tokens.finish().unwrap();
}

#[test]
fn folding_keeps_spans_on_the_nodes_it_wraps() {
    use crate::syntax::parser::node::AstNode;

    #[derive(Debug)]
    enum Tree {
        Leaf,
        Parent(Box<AstNode<Tree>>),
    }

    let grammar = AstNodeParser(symbol(Symbol::Dot).map(|_| Tree::Leaf))
        .fold(symbol(Symbol::Asterisk), |node, _| {
            Tree::Parent(Box::new(node))
        });
    let (value, tokens) = grammar
        .read(TokenStream::new(lexer::from_str(".** // end")))
        .unwrap();
    let outer = value.unwrap();
    assert_eq!(outer.span.unwrap().range, 0..3);
    let Tree::Parent(inner) = outer.value else {
        panic!("expected outer parent")
    };
    assert_eq!(inner.span.unwrap().range, 0..2);
    let Tree::Parent(leaf) = inner.value else {
        panic!("expected inner parent")
    };
    assert_eq!(leaf.span.unwrap().range, 0..1);
    assert!(matches!(leaf.value, Tree::Leaf));
    tokens.finish().unwrap();
}
