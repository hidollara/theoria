use super::super::{ParseError, Parser, Rule, defer, repeated, trivia};
use super::located;
use crate::ito::lexer::{self, Symbol, TokenKind, TokenStream};
use crate::ito::parser::tests::ReadFragment;
use crate::syntax::ast::{NodeSource, Trivia};
use std::cell::Cell;

thread_local! {
    static CONSTRUCTIONS: Cell<usize> = const { Cell::new(0) };
}

#[derive(Debug, PartialEq, Eq)]
struct Dot;

fn dot() -> impl Rule<Output = (Dot, NodeSource)> {
    CONSTRUCTIONS.set(CONSTRUCTIONS.get() + 1);
    located(DotRule)
}

struct DotRule;

impl Rule for DotRule {
    type Output = (Dot, Vec<Trivia>, crate::input::Cursor);

    fn description(&self) -> String {
        "a dot".into()
    }

    fn read(&self, parser: &mut Parser<'_, '_>) -> Result<Option<Self::Output>, ParseError> {
        let token = parser.next_if(|kind| matches!(kind, TokenKind::Symbol(Symbol::Dot)))?;
        Ok(token.map(|token| (Dot, Vec::new(), token.end)))
    }
}

#[test]
fn references_construct_the_target_only_when_reading_or_describing_it() {
    CONSTRUCTIONS.set(0);
    let rule = defer(dot);
    assert_eq!(CONSTRUCTIONS.get(), 0);
    assert_eq!(rule.description(), "a dot");
    assert_eq!(CONSTRUCTIONS.get(), 1);
    let (value, _) = rule
        .read_fragment(&mut lexer::from_str(" // note\n ."))
        .unwrap();
    let (dot, source) = value.unwrap();
    assert_eq!(dot, Dot);
    assert_eq!(source.start.offset..source.end.offset, 10..11);
    assert_eq!(CONSTRUCTIONS.get(), 2);
}

#[test]
fn direct_and_deferred_reads_return_the_same_located_node() {
    let source = ". // after";
    let mut stream = lexer::from_str(source);
    let mut direct = Parser::new(&mut stream);
    let mut stream = lexer::from_str(source);
    let mut deferred = Parser::new(&mut stream);
    let node = direct.read(dot()).unwrap();
    let (_, source) = &node;
    assert_eq!(source.start.offset..source.end.offset, 0..1);
    assert_eq!(node, deferred.read(defer(dot)).unwrap());
    assert_eq!(direct.read(repeated(trivia())).unwrap().len(), 2);
}

#[test]
fn empty_node_uses_the_current_token_start_for_both_boundaries() {
    let empty = located(super::super::rule("empty", |parser| {
        Ok(((), Vec::new(), parser.cursor()?))
    }));
    let mut tokens = lexer::from_str("  x");
    tokens.next().unwrap();
    let mut parser = Parser::new(&mut tokens);
    let ((), source) = parser.read(empty).unwrap();
    assert_eq!(source.start.offset..source.end.offset, 2..2);
    assert_eq!(parser.consumed(), 0);
}

#[test]
fn located_preserves_non_consuming_misses_and_lexical_errors() {
    let rule = dot();
    let mut stream = lexer::from_str(" ;");
    let (value, mut tokens) = rule.read_fragment(&mut stream).unwrap();
    assert_eq!(value, None);
    let token = tokens.next_if(|_| true).unwrap().unwrap();
    assert_eq!(token.kind, TokenKind::Symbol(Symbol::Semicolon));
    assert_eq!(token.start.offset, 1);

    let error = rule
        .read_fragment(&mut lexer::from_str(" \"unfinished"))
        .err()
        .unwrap();
    assert!(matches!(error, ParseError::Lexical(_)));
    assert_eq!(error.cursor().offset, 1);
}

#[test]
fn located_rule_starts_after_trivia_read_by_its_caller() {
    let mut tokens = lexer::from_str("// before\n. // after");
    let mut parser = Parser::new(&mut tokens);
    let before = parser.read(repeated(trivia())).unwrap();
    let (_, source) = parser.read(dot()).unwrap();
    let after = parser.read(repeated(trivia())).unwrap();

    assert_eq!(source.start.offset..source.end.offset, 10..11);
    assert!(source.trivia.is_empty());
    assert_eq!(before[0].start.offset..before[0].end.offset, 0..9);
    assert_eq!(after[1].start.offset..after[1].end.offset, 12..20);
}
