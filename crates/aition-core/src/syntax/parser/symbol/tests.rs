use super::super::{ParseError, Parser};
use super::symbol;
use crate::lexer::{self, LexError, Symbol};
use crate::syntax::parser::finish;
use crate::syntax::token::{TokenStream, TriviaKind};

#[test]
fn a_symbol_match_consumes_one_token_and_collects_leading_trivia_once() {
    let open = symbol(Symbol::OpenParen);
    let close = symbol(Symbol::CloseParen);
    assert_eq!(open.description(), "'('");
    assert_eq!(close.description(), "')'");

    let (value, tokens) = open
        .read(TokenStream::new(lexer::from_str("// note\n()")))
        .unwrap();
    assert_eq!(value, Some(()));
    let (value, tokens) = close.read(tokens).unwrap();
    assert_eq!(value, Some(()));
    let trivia = finish(tokens).unwrap();
    assert_eq!(trivia.len(), 2);
    assert_eq!(trivia[0].kind, TriviaKind::LineComment(" note".into()));
    assert_eq!(trivia[1].kind, TriviaKind::Whitespace("\n".into()));
}

#[test]
fn a_symbol_mismatch_preserves_symbols_words_and_strings() {
    let parser = symbol(Symbol::OpenParen);
    for source in [")", "word", "\"(\""] {
        let mut tokens = TokenStream::new(lexer::from_str(source));
        let mark = tokens.mark().unwrap();
        let (value, mut tokens) = parser.read(tokens).unwrap();
        assert_eq!(value, None);
        assert_eq!(tokens.span(&mark).range(), 0..0);
        assert_eq!(
            tokens.next_if(|_| true).unwrap().unwrap().span().range(),
            0..source.len()
        );
        finish(tokens).unwrap();
    }
}

#[test]
fn eof_is_a_mismatch_but_invalid_lexemes_are_errors() {
    let parser = symbol(Symbol::OpenParen);
    let (value, tokens) = parser
        .read(TokenStream::new(lexer::from_str(" // end")))
        .unwrap();
    assert_eq!(value, None);
    assert_eq!(finish(tokens).unwrap().len(), 2);

    let result = parser.read(TokenStream::new(lexer::from_str("\"unterminated")));
    assert!(matches!(
        result,
        Err(ParseError::Lexical(LexError::UnterminatedString { .. }))
    ));

    let result = parser.read(TokenStream::new(lexer::from_reader(&b"\xff"[..])));
    assert!(matches!(
        result,
        Err(ParseError::Lexical(LexError::Input(_)))
    ));
}
