use super::super::{Parser, symbol};
use super::repeated;
use crate::lexer::{self, Symbol};
use crate::syntax::parser::finish;
use crate::syntax::token::{TokenKind, TokenStream, TriviaKind};

#[test]
fn repetition_stops_before_the_next_construct_and_collects_trivia_once() {
    let parser = repeated(symbol(Symbol::Dot));
    let (values, mut tokens) = parser
        .read(TokenStream::new(lexer::from_str(
            ". // middle\n. // end\n;",
        )))
        .unwrap();
    assert_eq!(values.unwrap().len(), 2);
    assert!(
        tokens
            .next_if(|kind| matches!(kind, TokenKind::Symbol(Symbol::Semicolon)))
            .unwrap()
            .is_some()
    );
    let trivia = finish(tokens).unwrap();
    let comments: Vec<_> = trivia
        .iter()
        .filter_map(|item| match &item.kind {
            TriviaKind::LineComment(text) => Some(text.as_ref()),
            _ => None,
        })
        .collect();
    assert_eq!(comments, [" middle", " end"]);
}

#[test]
#[should_panic(expected = "a repeated parser must consume a token")]
fn repetition_rejects_empty_success_instead_of_looping_forever() {
    let _ = repeated(repeated(symbol(Symbol::Dot))).read(TokenStream::new(lexer::from_str("")));
}
