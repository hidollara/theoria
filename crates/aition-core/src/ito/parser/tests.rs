mod name;

use super::{ParseError, Parser, Rule, repeated, trivia};
use crate::ito::lexer::{self, LexError, TokenKind, TokenStream, TokenStreamItem};
use crate::syntax::ast::{AstNode, NodeSource, TriviaKind};

pub(super) fn read_complete<'s, T>(
    mut tokens: impl TokenStream<'s>,
    rule: impl Rule<Output = (T, NodeSource)>,
) -> Result<AstNode<T>, ParseError> {
    let mut parser = Parser::new(&mut tokens);
    let node = parser.read(rule)?;
    if let TokenStreamItem::Token(next) = parser.peek()? {
        return Err(ParseError::expected("end of input".into(), next.start));
    }
    Ok(node.into())
}

fn node_source<T>(node: &AstNode<T>) -> &NodeSource {
    node.source
        .as_ref()
        .expect("parsed node must have source information")
}

fn node_bytes<T>(node: &AstNode<T>) -> std::ops::Range<usize> {
    let source = node_source(node);
    source.start.offset..source.end.offset
}

impl<'source, F> TokenStream<'source> for F
where
    F: FnMut() -> Result<TokenStreamItem<'source>, LexError>,
{
    fn next(&mut self) -> Result<TokenStreamItem<'source>, LexError> {
        self()
    }
}

#[test]
fn reading_trivia_stops_at_eoi_or_the_next_significant_token() {
    let mut stream = lexer::from_str(" // end\r\n");
    let mut parser = Parser::new(&mut stream);
    let attached = parser.read(repeated(trivia())).unwrap();
    assert!(matches!(parser.peek().unwrap(), TokenStreamItem::Eoi(_)));
    assert_eq!(attached.len(), 3);
    assert_eq!(attached[1].kind, TriviaKind::LineComment(" end".into()));

    let mut stream = lexer::from_str(" // comment\n word");
    let mut parser = Parser::new(&mut stream);
    parser.read(repeated(trivia())).unwrap();
    let TokenStreamItem::Token(next) = parser.peek().unwrap() else {
        panic!("expected a word");
    };
    assert_eq!(next.kind, TokenKind::Word("word".into()));
    assert_eq!(
        (next.start.offset, next.start.line, next.start.column),
        (13, 2, 2)
    );
}

#[test]
fn expectations_locate_the_next_token_or_eoi_after_trivia() {
    let mut stream = lexer::from_str(" ;");
    let mut tokens = Parser::new(&mut stream);
    tokens.read(repeated(trivia())).unwrap();
    let error = ParseError::expected("')'".into(), tokens.cursor().unwrap());
    assert_eq!(error.to_string(), "expected ')' at line 1, column 2");
    assert_eq!(error.cursor().offset, 1);
    let token = tokens.next_if(|_| true).unwrap().unwrap();
    assert_eq!(token.start.offset..token.end.offset, 1..2);

    let source = " // end\r\n";
    let mut stream = lexer::from_str(source);
    let mut tokens = Parser::new(&mut stream);
    tokens.read(repeated(trivia())).unwrap();
    let error = ParseError::expected("a type".into(), tokens.cursor().unwrap());
    assert!(matches!(error, ParseError::Unexpected { expected, cursor }
        if cursor.offset == source.len() && (cursor.line, cursor.column) == (2, 1)
        && expected == "a type"));
}

#[test]
fn reading_trivia_and_peeking_preserve_lexical_errors() {
    let mut stream = lexer::from_str(" \"unfinished");
    let error = Parser::new(&mut stream)
        .read(repeated(trivia()))
        .unwrap_err();
    assert!(matches!(
        error,
        ParseError::Lexical(LexError::UnterminatedString { .. })
    ));
    assert_eq!(error.cursor().offset, 1);

    let mut stream = lexer::from_reader(&b"\xff"[..]);
    let mut tokens = Parser::new(&mut stream);
    let error = tokens.peek().unwrap_err();
    assert!(matches!(error, ParseError::Lexical(LexError::Input(_))));
}

pub(super) trait ReadFragment: super::Rule {
    fn read_fragment<'s, 'input>(
        &self,
        tokens: &'input mut dyn TokenStream<'s>,
    ) -> Result<(Option<Self::Output>, super::Parser<'s, 'input>), super::ParseError> {
        let mut parser = Parser::new(tokens);
        parser.read(repeated(trivia()))?;
        let value = self.read(&mut parser)?;
        Ok((value, parser))
    }
}

impl<R: super::Rule> ReadFragment for R {}

#[test]
fn trivia_matches_one_token_and_leaves_a_word_or_eoi_untouched() {
    let mut stream = lexer::from_str("  // end\nf");
    let mut parser = Parser::new(&mut stream);
    for expected in [
        TriviaKind::Whitespace("  ".into()),
        TriviaKind::LineComment(" end".into()),
        TriviaKind::Whitespace("\n".into()),
    ] {
        assert_eq!(parser.read(trivia()).unwrap().kind, expected);
    }
    assert!(trivia().read(&mut parser).unwrap().is_none());
    let word = parser.next_if(|_| true).unwrap().unwrap();
    assert_eq!(word.kind, TokenKind::Word("f".into()));
    assert!(trivia().read(&mut parser).unwrap().is_none());
    assert!(matches!(parser.peek().unwrap(), TokenStreamItem::Eoi(_)));
}
