use super::super::{rule, symbol, trivia};
use super::repeated;
use crate::ito::lexer::{self, Symbol, TokenKind, TokenStreamItem};
use crate::ito::parser::tests::ReadFragment;
use crate::syntax::ast::TriviaKind;

#[test]
fn repetition_stops_before_the_next_construct_and_collects_trivia_once() {
    let rule = repeated(rule("'.'", |parser| {
        parser.read(symbol(Symbol::Dot))?;
        parser.read(repeated(trivia()))
    }));
    let mut stream = lexer::from_str(". // middle\n. // end\n;");
    let (values, mut tokens) = rule.read_fragment(&mut stream).unwrap();
    let values = values.unwrap();
    assert_eq!(values.len(), 2);
    assert!(
        tokens
            .next_if(|kind| matches!(kind, TokenKind::Symbol(Symbol::Semicolon)))
            .unwrap()
            .is_some()
    );
    assert!(matches!(tokens.peek().unwrap(), TokenStreamItem::Eoi(_)));
    let trivia: Vec<_> = values.into_iter().flatten().collect();
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
#[should_panic(expected = "a repeated rule must consume a token")]
fn repetition_rejects_empty_success_instead_of_looping_forever() {
    let _ = repeated(repeated(symbol(Symbol::Dot))).read_fragment(&mut lexer::from_str(""));
}
