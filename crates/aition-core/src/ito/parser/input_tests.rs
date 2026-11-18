use super::{ParseError, Parser, Rule, repeated, trivia};
use crate::ito::lexer::{self, Symbol, TokenKind, TokenStream, TokenStreamItem};
use std::{borrow::Cow, cell::Cell};

struct SymbolRule(Symbol);

impl Rule for SymbolRule {
    type Output = ();

    fn description(&self) -> String {
        self.0.to_string()
    }

    fn read(&self, parser: &mut Parser<'_, '_>) -> Result<Option<()>, ParseError> {
        Ok(parser
            .next_if(|kind| matches!(kind, TokenKind::Symbol(s) if *s == self.0))?
            .map(|_| ()))
    }
}

#[test]
fn required_reads_reuse_a_rule_and_report_missing_input() {
    let dot = SymbolRule(Symbol::Dot);
    let mut stream = lexer::from_str(". .;");
    let mut parser = Parser::new(&mut stream);
    parser.read(&dot).unwrap();
    parser.read(repeated(trivia())).unwrap();
    parser.read(&dot).unwrap();
    let error = parser.read(&dot).unwrap_err();
    assert!(matches!(error, ParseError::Unexpected { expected, cursor }
        if expected == "." && cursor.offset == 3));
    assert_eq!(parser.consumed(), 3);
    assert_eq!(
        parser.next_if(|_| true).unwrap().unwrap().kind,
        TokenKind::Symbol(Symbol::Semicolon)
    );
    assert!(matches!(parser.peek().unwrap(), TokenStreamItem::Eoi(_)));
}

#[test]
fn leaves_do_not_consume_trivia_without_an_allowed_boundary() {
    for source in [" .", "// note\n."] {
        let mut stream = lexer::from_str(source);
        let mut parser = Parser::new(&mut stream);
        assert!(SymbolRule(Symbol::Dot).read(&mut parser).unwrap().is_none());
        assert_eq!(parser.consumed(), 0);
        let gap = parser.read(repeated(trivia())).unwrap();
        assert!(!gap.is_empty());
        assert!(SymbolRule(Symbol::Dot).read(&mut parser).unwrap().is_some());
        assert!(matches!(parser.peek().unwrap(), TokenStreamItem::Eoi(_)));
    }
}

#[test]
fn every_token_kind_advances_the_consumed_position() {
    let mut stream = lexer::from_str(" // gap\nx");
    let mut parser = Parser::new(&mut stream);
    for (index, end) in [1, 7, 8, 9].into_iter().enumerate() {
        let token = parser.next_if(|_| true).unwrap().unwrap();
        assert_eq!(token.end.offset, end);
        assert_eq!(parser.consumed(), index + 1);
    }
    assert!(matches!(parser.peek().unwrap(), TokenStreamItem::Eoi(_)));
}

#[test]
fn lookahead_preserves_trivia_and_input_boundaries() {
    let source = " // comment\r\nword";
    let mut stream = lexer::from_str(source);
    let mut parser = Parser::new(&mut stream);
    for expected in [
        TokenKind::Whitespace(" ".into()),
        TokenKind::LineComment(" comment".into()),
        TokenKind::Whitespace("\r\n".into()),
        TokenKind::Word("word".into()),
    ] {
        assert_eq!(parser.next_if(|_| true).unwrap().unwrap().kind, expected);
    }
    let TokenStreamItem::Eoi(cursor) = parser.peek().unwrap() else {
        panic!("expected end of input");
    };
    assert_eq!(
        (cursor.offset, cursor.line, cursor.column),
        (source.len(), 2, 5)
    );
}

#[test]
fn peek_and_mismatch_do_not_consume_or_read_twice() {
    let reads = Cell::new(0);
    let mut tokens = lexer::from_str("a");
    let mut source = || {
        reads.set(reads.get() + 1);
        tokens.next()
    };
    let mut parser = Parser::new(&mut source);
    assert_eq!(reads.get(), 0);
    for _ in 0..3 {
        assert_eq!(parser.cursor().unwrap().offset, 0);
        assert!(parser.next_if(|_| false).unwrap().is_none());
    }
    assert_eq!(reads.get(), 1);
    assert!(parser.next_if(|_| true).unwrap().is_some());
    for _ in 0..3 {
        assert!(matches!(parser.peek().unwrap(), TokenStreamItem::Eoi(_)));
        assert!(
            parser
                .next_if(|kind| matches!(kind, TokenKind::Word(_)))
                .unwrap()
                .is_none()
        );
    }
    assert_eq!(reads.get(), 2);
}

#[test]
fn empty_input_starts_at_eoi() {
    let mut stream = lexer::from_str("");
    let mut parser = Parser::new(&mut stream);
    let TokenStreamItem::Eoi(end) = parser.peek().unwrap() else {
        panic!("expected end of input");
    };
    assert_eq!((end.offset, end.line, end.column), (0, 1, 1));
}

#[test]
fn eoi_position_comes_from_the_token_stream() {
    let mut tokens = lexer::from_str("x  ");
    let mut source = move || {
        loop {
            match tokens.next()? {
                TokenStreamItem::Token(token) if matches!(token.kind, TokenKind::Whitespace(_)) => {
                    continue;
                }
                item => return Ok(item),
            }
        }
    };
    let mut parser = Parser::new(&mut source);
    let word = parser
        .next_if(|kind| matches!(kind, TokenKind::Word(_)))
        .unwrap()
        .unwrap();
    assert_eq!(word.end.offset, 1);
    assert_eq!(parser.cursor().unwrap().offset, 3);
    assert!(matches!(parser.peek().unwrap(), TokenStreamItem::Eoi(_)));
}

#[test]
fn eoi_skips_the_predicate_and_does_not_advance() {
    let mut stream = lexer::from_str("");
    let mut parser = Parser::new(&mut stream);
    for _ in 0..3 {
        assert!(
            parser
                .next_if(|_| panic!("end of input has no token kind"))
                .unwrap()
                .is_none()
        );
        assert_eq!(parser.consumed(), 0);
        assert!(matches!(parser.peek().unwrap(), TokenStreamItem::Eoi(_)));
    }
    assert!(matches!(parser.peek().unwrap(), TokenStreamItem::Eoi(_)));
}

#[test]
fn token_text_ownership_is_preserved() {
    let mut stream = lexer::from_str("\"text\"");
    let mut borrowed = Parser::new(&mut stream);
    let token = borrowed.next_if(|_| true).unwrap().unwrap();
    assert!(matches!(
        token.kind,
        TokenKind::String(Cow::Borrowed("text"))
    ));
    assert_eq!(token.start.offset..token.end.offset, 0..6);
    assert_eq!(token.content_start().offset, 1);

    let mut stream = lexer::from_reader(&b"\"text\""[..]);
    let mut owned = Parser::new(&mut stream);
    let token = owned.next_if(|_| true).unwrap().unwrap();
    assert!(matches!(token.kind, TokenKind::String(Cow::Owned(_))));
    assert_eq!(token.start.offset..token.end.offset, 0..6);
}

#[test]
fn lexical_errors_are_returned_without_reclassification() {
    let mut stream = lexer::from_str("\"unfinished");
    let mut parser = Parser::new(&mut stream);
    let error = parser.peek().unwrap_err();
    assert!(matches!(
        error,
        ParseError::Lexical(crate::ito::lexer::LexError::UnterminatedString { .. })
    ));
    assert_eq!(error.cursor().offset, 0);

    let mut stream = lexer::from_reader(&b"\xff"[..]);
    let mut parser = Parser::new(&mut stream);
    assert!(matches!(
        parser.peek(),
        Err(ParseError::Lexical(crate::ito::lexer::LexError::Input(_)))
    ));
}
