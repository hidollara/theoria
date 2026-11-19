use super::super::{ParseError, Parser, finish, sequence, symbol};
use crate::lexer::{self, Symbol};
use crate::syntax::token::{TokenKind, TokenStream};

#[test]
fn candidates_are_tried_in_order_and_stop_at_the_first_match() {
    let parser = choice![
        symbol(Symbol::Dot).map(|()| "dot"),
        sequence![symbol(Symbol::OpenParen), symbol(Symbol::CloseParen)].map(|_| "parentheses"),
        symbol(Symbol::Comma).map(|()| "comma"),
        symbol(Symbol::Comma).map(|()| panic!("later matches must not be read")),
    ];
    for (source, expected) in [(".", "dot"), ("()", "parentheses"), (",", "comma")] {
        let (value, tokens) = parser
            .read(TokenStream::new(lexer::from_str(source)))
            .unwrap();
        assert_eq!(value, Some(expected));
        finish(tokens).unwrap();
    }
}

#[test]
fn a_complete_miss_preserves_input_and_describes_all_candidates() {
    let parser = choice![
        symbol(Symbol::Dot),
        symbol(Symbol::Comma),
        symbol(Symbol::Semicolon)
    ];
    assert_eq!(parser.description(), "'.', ',', or ';'");
    let mut tokens = TokenStream::new(lexer::from_str(" // note\nword"));
    let mark = tokens.mark().unwrap();
    let (value, mut tokens) = parser.read(tokens).unwrap();
    assert_eq!(value, None);
    assert_eq!(tokens.span(&mark).range(), 9..9);
    let token = tokens
        .next_if(|kind| matches!(kind, TokenKind::Word(_)))
        .unwrap()
        .unwrap();
    assert_eq!(token.kind, TokenKind::Word("word".into()));
    assert_eq!(finish(tokens).unwrap().len(), 3);
}

#[test]
fn an_error_does_not_try_another_candidate() {
    let parser = choice![
        sequence![symbol(Symbol::OpenParen), symbol(Symbol::CloseParen)].map(|_| ()),
        symbol(Symbol::CloseBracket).map(|()| panic!("a partial match must commit")),
    ];
    let error = parser
        .read(TokenStream::new(lexer::from_str("(]")))
        .err()
        .unwrap();
    assert!(matches!(error, ParseError::Unexpected { expected, cursor }
        if expected == "')'" && cursor.offset == 1));
    let error = parser
        .read(TokenStream::new(lexer::from_str("\"unfinished")))
        .err()
        .unwrap();
    assert!(matches!(error, ParseError::Lexical(_)));
}

#[test]
fn a_single_candidate_accepts_a_trailing_comma() {
    let parser = choice![symbol(Symbol::Dot).map(|()| 7),];
    assert_eq!(parser.description(), "'.'");
    let (value, tokens) = parser.read(TokenStream::new(lexer::from_str("."))).unwrap();
    assert_eq!(value, Some(7));
    finish(tokens).unwrap();
}

#[test]
fn descriptions_use_or_only_before_the_last_candidate() {
    assert_eq!(
        choice![symbol(Symbol::Dot), symbol(Symbol::Comma)].description(),
        "'.' or ','",
    );
    assert_eq!(
        choice![
            symbol(Symbol::Dot),
            symbol(Symbol::Comma),
            symbol(Symbol::Semicolon),
            symbol(Symbol::Colon),
        ]
        .description(),
        "'.', ',', ';', or ':'",
    );
}
