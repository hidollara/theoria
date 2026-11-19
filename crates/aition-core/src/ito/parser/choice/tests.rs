use super::super::{ParseError, Rule, repeated, rule, symbol, trivia};
use crate::ito::lexer::{self, Symbol, TokenKind, TokenStreamItem};
use crate::ito::parser::Parser;
use crate::ito::parser::tests::ReadFragment;

#[test]
fn candidates_are_tried_in_order_and_stop_at_the_first_match() {
    let rule = choice![
        symbol(Symbol::Dot).map(|_| "dot"),
        rule("'('", |parser| {
            parser.read(symbol(Symbol::OpenParen))?;
            parser.read(symbol(Symbol::CloseParen))?;
            Ok("parentheses")
        }),
        symbol(Symbol::Comma).map(|_| "comma"),
        symbol(Symbol::Comma).map(|_| panic!("later matches must not be read")),
    ];
    for (source, expected) in [(".", "dot"), ("()", "parentheses"), (",", "comma")] {
        let mut stream = lexer::from_str(source);
        let (value, mut tokens) = rule.read_fragment(&mut stream).unwrap();
        assert_eq!(value, Some(expected));
        assert!(matches!(tokens.peek().unwrap(), TokenStreamItem::Eoi(_)));
    }
}

#[test]
fn a_complete_miss_preserves_input_and_describes_all_candidates() {
    let rule = choice![
        symbol(Symbol::Dot),
        symbol(Symbol::Comma),
        symbol(Symbol::Semicolon)
    ];
    assert_eq!(rule.description(), "'.', ',', or ';'");
    let mut stream = lexer::from_str(" // note\nword");
    let mut tokens = Parser::new(&mut stream);
    let gap = tokens.read(repeated(trivia())).unwrap();
    let consumed = tokens.consumed();
    let value = rule.read(&mut tokens).unwrap();
    assert_eq!(value, None);
    assert_eq!(tokens.consumed(), consumed);
    let token = tokens
        .next_if(|kind| matches!(kind, TokenKind::Word(_)))
        .unwrap()
        .unwrap();
    assert_eq!(token.kind, TokenKind::Word("word".into()));
    assert!(matches!(tokens.peek().unwrap(), TokenStreamItem::Eoi(_)));
    assert_eq!(gap.len(), 3);
}

#[test]
fn an_error_does_not_try_another_candidate() {
    let rule = choice![
        rule("'('", |parser| {
            parser.read(symbol(Symbol::OpenParen))?;
            parser.read(symbol(Symbol::CloseParen))
        }),
        symbol(Symbol::CloseBracket).map(|_| panic!("a partial match must commit")),
    ];
    let error = rule
        .read_fragment(&mut lexer::from_str("(]"))
        .err()
        .unwrap();
    assert!(matches!(error, ParseError::Unexpected { expected, cursor }
        if expected == "')'" && cursor.offset == 1));
    let error = rule
        .read_fragment(&mut lexer::from_str("\"unfinished"))
        .err()
        .unwrap();
    assert!(matches!(error, ParseError::Lexical(_)));
}

#[test]
fn a_single_candidate_accepts_a_trailing_comma() {
    let rule = choice![symbol(Symbol::Dot).map(|_| 7),];
    assert_eq!(rule.description(), "'.'");
    let mut stream = lexer::from_str(".");
    let (value, mut tokens) = rule.read_fragment(&mut stream).unwrap();
    assert_eq!(value, Some(7));
    assert!(matches!(tokens.peek().unwrap(), TokenStreamItem::Eoi(_)));
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
