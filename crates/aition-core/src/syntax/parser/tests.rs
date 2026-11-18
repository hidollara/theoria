use super::{ParseError, finish};
use crate::lexer::{self, LexError};
use crate::syntax::token::{TokenStream, TriviaKind};

#[test]
fn completion_collects_trailing_trivia_but_rejects_remaining_tokens() {
    let trivia = finish(TokenStream::new(lexer::from_str(" // end\r\n"))).unwrap();
    assert_eq!(trivia.len(), 3);
    assert_eq!(trivia[1].kind, TriviaKind::LineComment(" end".into()));

    let error = finish(TokenStream::new(lexer::from_str(" // comment\n word"))).unwrap_err();
    assert!(matches!(error, ParseError::Unexpected { expected, cursor }
        if cursor.offset == 13 && (cursor.line, cursor.column) == (2, 2)
        && expected == "end of input"));
}

#[test]
fn expectations_locate_the_next_token_or_eof_after_trivia() {
    let mut tokens = TokenStream::new(lexer::from_str(" ;"));
    let error = ParseError::expected("')'".into(), tokens.peek().unwrap().cursor());
    assert_eq!(error.to_string(), "expected ')' at line 1, column 2");
    assert_eq!(error.cursor().offset, 1);
    assert_eq!(
        tokens.next_if(|_| true).unwrap().unwrap().span().range(),
        1..2
    );

    let source = " // end\r\n";
    let mut tokens = TokenStream::new(lexer::from_str(source));
    let error = ParseError::expected("a type".into(), tokens.peek().unwrap().cursor());
    assert!(matches!(error, ParseError::Unexpected { expected, cursor }
        if cursor.offset == source.len() && (cursor.line, cursor.column) == (2, 1)
        && expected == "a type"));
}

#[test]
fn completion_preserves_lexical_errors() {
    let error = finish(TokenStream::new(lexer::from_str(" \"unfinished"))).unwrap_err();
    assert!(matches!(
        error,
        ParseError::Lexical(LexError::UnterminatedString { .. })
    ));
    assert_eq!(error.cursor().offset, 1);

    let tokens = TokenStream::new(lexer::from_reader(&b"\xff"[..]));
    let error = finish(tokens).unwrap_err();
    assert!(matches!(error, ParseError::Lexical(LexError::Input(_))));
}
