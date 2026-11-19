use super::super::{ParseError, Parser, repeated, symbol};
use crate::lexer::{self, Symbol};
use crate::syntax::parser::finish;
use crate::syntax::token::{TokenKind, TokenStream};

#[test]
fn a_sequence_distinguishes_an_initial_miss_from_an_incomplete_match() {
    let parser = sequence![symbol(Symbol::OpenParen), symbol(Symbol::CloseParen)];
    let (value, mut tokens) = parser.read(TokenStream::new(lexer::from_str("["))).unwrap();
    assert_eq!(value, None);
    assert!(
        tokens
            .next_if(|kind| matches!(kind, TokenKind::Symbol(Symbol::OpenBracket)))
            .unwrap()
            .is_some()
    );
    finish(tokens).unwrap();

    let error = parser
        .read(TokenStream::new(lexer::from_str("(]")))
        .err()
        .unwrap();
    assert!(matches!(error, ParseError::Unexpected { expected, cursor }
        if expected == "')'" && cursor.offset == 1));
}

#[test]
fn an_empty_first_match_does_not_commit_a_sequence() {
    let parser = sequence![repeated(symbol(Symbol::Dot)), symbol(Symbol::Comma)];
    let (value, mut tokens) = parser.read(TokenStream::new(lexer::from_str(";"))).unwrap();
    assert_eq!(value, None);
    assert!(
        tokens
            .next_if(|kind| matches!(kind, TokenKind::Symbol(Symbol::Semicolon)))
            .unwrap()
            .is_some()
    );
    finish(tokens).unwrap();
}

#[test]
fn three_parsers_return_a_flat_tuple_in_source_order() {
    let parser = sequence![
        symbol(Symbol::OpenParen).map(|()| "open"),
        repeated(symbol(Symbol::Dot)).map(|dots| dots.len()),
        symbol(Symbol::CloseParen).map(|()| true),
    ];
    let (value, tokens) = parser
        .read(TokenStream::new(lexer::from_str("(..)")))
        .unwrap();
    assert_eq!(value, Some(("open", 2, true)));
    finish(tokens).unwrap();

    for (source, offset) in [("(..]", 3), ("(]", 1)] {
        let error = parser
            .read(TokenStream::new(lexer::from_str(source)))
            .err()
            .unwrap();
        assert!(matches!(error, ParseError::Unexpected { expected, cursor }
            if expected == "')'" && cursor.offset == offset));
    }
}

#[test]
fn long_sequences_construct_parsers_once_and_preserve_every_result() {
    let constructed = std::cell::Cell::new(0);
    let next = || {
        let index = constructed.get();
        constructed.set(index + 1);
        symbol(Symbol::Dot).map(move |()| index)
    };
    let parser = sequence![
        next(),
        next(),
        next(),
        next(),
        next(),
        next(),
        next(),
        next(),
        next(),
        next(),
        next(),
        next(),
        next(),
        next(),
        next(),
        next(),
    ];
    assert_eq!(constructed.get(), 16);
    for _ in 0..2 {
        let (value, tokens) = parser
            .read(TokenStream::new(lexer::from_str("................")))
            .unwrap();
        let (a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p) = value.unwrap();
        assert_eq!(
            [a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p],
            std::array::from_fn(|index| index),
        );
        finish(tokens).unwrap();
    }
    assert_eq!(constructed.get(), 16);
}

#[test]
fn single_and_nested_sequences_preserve_explicit_grouping() {
    let parser = sequence![
        sequence![symbol(Symbol::Dot).map(|()| 1)],
        sequence![symbol(Symbol::Comma).map(|()| "comma"),],
    ];
    let (value, tokens) = parser
        .read(TokenStream::new(lexer::from_str(".,")))
        .unwrap();
    assert_eq!(value, Some(((1,), ("comma",))));
    finish(tokens).unwrap();
}
