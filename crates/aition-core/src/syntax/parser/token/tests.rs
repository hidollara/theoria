use std::borrow::Cow;

use crate::lexer::{self, LexemeIterator, Symbol};

use super::{TokenKind, TokenStream};
use crate::syntax::parser::{Expected, ParseError, TriviaKind};

#[test]
fn peeking_and_consuming_do_not_interpret_words_or_duplicate_trivia() {
    let mut input = TokenStream::new(lexer::from_str("// note\n123invalid "));
    for _ in 0..3 {
        let token = input.peek().unwrap().unwrap();
        assert_eq!(token.kind, TokenKind::Word("123invalid".into()));
        assert_eq!(token.span(), 8..18);
    }
    assert_eq!(input.next().unwrap().unwrap().span(), 8..18);
    assert!(input.next().unwrap().is_none());
    assert!(input.peek().unwrap().is_none());
    let trivia: Vec<_> = input
        .finish()
        .unwrap()
        .into_iter()
        .map(|item| item.kind)
        .collect();
    assert_eq!(
        trivia,
        [
            TriviaKind::LineComment(" note".into()),
            TriviaKind::Whitespace("\n".into()),
            TriviaKind::Whitespace(" ".into()),
        ]
    );
}

#[test]
fn mismatched_symbols_leave_the_next_token_unconsumed() {
    let mut input = TokenStream::new(lexer::from_str(" ;"));
    assert!(!input.eat(Symbol::Comma).unwrap());
    let error = input.error(Expected::Symbol(Symbol::CloseParen));
    let ParseError::Unexpected { span, expected, .. } = error else {
        panic!("expected a symbol mismatch");
    };
    assert_eq!(span, 1..2);
    assert_eq!(expected, Expected::Symbol(Symbol::CloseParen));
    assert!(input.eat(Symbol::Semicolon).unwrap());
    assert_eq!(input.finish().unwrap().len(), 1);
}

#[test]
fn grammar_tokens_preserve_text_positions_and_ownership() {
    let source = "// c\nword \"x//y\" ; // end";
    check(lexer::from_str(source), true);
    check(lexer::from_reader(source.as_bytes()), false);

    fn check<'source>(lexemes: impl LexemeIterator<'source>, borrowed: bool) {
        let mut input = TokenStream::new(lexemes);
        for (expected_kind, expected_text, expected_span) in [
            ("word", "word", 5..9),
            ("string", "x//y", 10..16),
            ("symbol", ";", 17..18),
        ] {
            let token = input.next().unwrap().unwrap();
            assert_eq!(token.span(), expected_span);
            // Exhaustive: trivia is not representable in a grammar token.
            let (kind, text) = match token.kind {
                TokenKind::Word(text) => ("word", text),
                TokenKind::String(text) => ("string", text),
                TokenKind::Symbol(symbol) => ("symbol", Cow::Owned(symbol.to_string())),
            };
            assert_eq!((kind, text.as_ref()), (expected_kind, expected_text));
            if kind != "symbol" {
                assert_eq!(matches!(text, Cow::Borrowed(_)), borrowed);
            }
        }
        let trivia = input.finish().unwrap();
        let comments: Vec<_> = trivia
            .iter()
            .filter_map(|item| match &item.kind {
                TriviaKind::LineComment(text) => Some(text.as_ref()),
                _ => None,
            })
            .collect();
        assert_eq!(comments, [" c", " end"]);
    }
}
