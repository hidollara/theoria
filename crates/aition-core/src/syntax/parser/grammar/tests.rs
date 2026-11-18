use super::{Parser, symbol};
use crate::lexer::{self, Symbol};
use crate::syntax::parser::token::TokenStream;

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
