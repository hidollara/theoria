use super::super::{ParseError, Rule, repeated, trivia};
use super::symbol;
use crate::ito::lexer::{self, LexError, Symbol, TokenStreamItem};
use crate::ito::parser::Parser;
use crate::ito::parser::tests::ReadFragment;
use crate::syntax::ast::TriviaKind;

#[test]
fn a_symbol_match_accepts_leading_trivia_at_the_input_boundary() {
    let open = symbol(Symbol::OpenParen);
    let close = symbol(Symbol::CloseParen);
    assert_eq!(open.description(), "'('");
    assert_eq!(close.description(), "')'");

    let mut stream = lexer::from_str("// note\n()");
    let mut tokens = Parser::new(&mut stream);
    let leading = tokens.read(repeated(trivia())).unwrap();
    let value = open.read(&mut tokens).unwrap();
    assert_eq!(value.unwrap().offset, 9);
    let value = close.read(&mut tokens).unwrap();
    assert_eq!(value.unwrap().offset, 10);
    assert!(matches!(tokens.peek().unwrap(), TokenStreamItem::Eoi(_)));
    let trivia = leading;
    assert_eq!(trivia.len(), 2);
    assert_eq!(trivia[0].kind, TriviaKind::LineComment(" note".into()));
    assert_eq!(trivia[1].kind, TriviaKind::Whitespace("\n".into()));
}

#[test]
fn a_symbol_mismatch_preserves_symbols_words_and_strings() {
    let rule = symbol(Symbol::OpenParen);
    for source in [")", "word", "\"(\""] {
        let mut stream = lexer::from_str(source);
        let mut tokens = Parser::new(&mut stream);
        let consumed = tokens.consumed();
        let value = rule.read(&mut tokens).unwrap();
        assert_eq!(value, None);
        assert_eq!(tokens.consumed(), consumed);
        let token = tokens.next_if(|_| true).unwrap().unwrap();
        assert_eq!(token.start.offset..token.end.offset, 0..source.len());
        assert!(matches!(tokens.peek().unwrap(), TokenStreamItem::Eoi(_)));
    }
}

#[test]
fn eoi_is_a_mismatch_but_invalid_tokens_are_errors() {
    let rule = symbol(Symbol::OpenParen);
    let mut stream = lexer::from_str(" // end");
    let (value, mut tokens) = rule.read_fragment(&mut stream).unwrap();
    assert_eq!(value, None);
    assert!(matches!(tokens.peek().unwrap(), TokenStreamItem::Eoi(_)));

    let mut stream = lexer::from_str("\"unterminated");
    let result = rule.read_fragment(&mut stream);
    assert!(matches!(
        result,
        Err(ParseError::Lexical(LexError::UnterminatedString { .. }))
    ));

    let mut stream = lexer::from_reader(&b"\xff"[..]);
    let result = rule.read_fragment(&mut stream);
    assert!(matches!(
        result,
        Err(ParseError::Lexical(LexError::Input(_)))
    ));
}
