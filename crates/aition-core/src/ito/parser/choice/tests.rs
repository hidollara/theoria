use crate::ito::lexer::{self, LexError};
use crate::ito::parser::Rule;
use crate::ito::parser::error::ParseError;
use crate::ito::parser::state::Parser;
use crate::ito::parser::trivia::trivia;

fn keyword(name: &'static str) -> impl Rule<Output = &'static str> {
    move |parser: &mut Parser<'_, '_>| match parser.keyword(name)? {
        Some(_) => Ok(name),
        None => Err(ParseError::expected_keyword(name, parser.cursor()?)),
    }
}

#[test]
fn all_misses_report_the_candidates_in_order_without_consuming_input() {
    let mut tokens = lexer::from_str(" unknown");
    let mut parser = Parser::new(&mut tokens);
    parser.whitespace().unwrap().unwrap();

    let error = choice![keyword("type"), keyword("module")]
        .apply(&mut parser)
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        r#"expected "type" or "module" at line 1, column 2"#
    );

    let error = choice![keyword("type"), keyword("module"), keyword("alias"),]
        .apply(&mut parser)
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        r#"expected "type", "module", or "alias" at line 1, column 2"#
    );
    assert_eq!(parser.word().unwrap().unwrap().value.as_ref(), "unknown");
}

#[test]
fn trivia_matches_but_only_keywords_appear_in_the_choice_diagnostic() {
    let mut tokens = lexer::from_str("// note\nunknown");
    let mut parser = Parser::new(&mut tokens);
    let rule = choice![
        #[silent]
        trivia.map(|_| "trivia"),
        keyword("type"),
        #[silent]
        keyword("hidden"),
        keyword("module"),
        keyword("alias"),
        #[silent]
        keyword("other"),
    ];

    assert_eq!(rule.apply(&mut parser).unwrap(), "trivia");
    assert_eq!(rule.apply(&mut parser).unwrap(), "trivia");
    let error = rule.apply(&mut parser).unwrap_err();
    assert_eq!(
        error.to_string(),
        r#"expected "type", "module", or "alias" at line 2, column 1"#
    );
}

#[test]
fn separators_count_only_reported_candidates() {
    let mut tokens = lexer::from_str("unknown");
    let mut parser = Parser::new(&mut tokens);
    let two = choice![
        keyword("type"),
        #[silent]
        keyword("hidden"),
        keyword("alias"),
        #[silent]
        keyword("other"),
    ];
    let error = two.apply(&mut parser).unwrap_err();
    assert_eq!(
        error.to_string(),
        r#"expected "type" or "alias" at line 1, column 1"#
    );

    let one = choice![
        #[silent]
        keyword("hidden"),
        keyword("type"),
        #[silent]
        keyword("other"),
    ];
    let error = one.apply(&mut parser).unwrap_err();
    assert_eq!(error.to_string(), r#"expected "type" at line 1, column 1"#);
}

#[test]
fn an_all_silent_choice_can_match_and_contributes_no_expectation() {
    let mut tokens = lexer::from_str("module unknown");
    let mut parser = Parser::new(&mut tokens);
    let silent = choice![
        #[silent]
        keyword("type"),
        #[silent]
        keyword("module"),
    ];
    assert_eq!(silent.apply(&mut parser).unwrap(), "module");
    parser.whitespace().unwrap().unwrap();
    let error = silent.apply(&mut parser).unwrap_err();
    assert_eq!(error.to_string(), "unexpected input at line 1, column 8");

    let error = choice![silent, keyword("alias")]
        .apply(&mut parser)
        .unwrap_err();
    assert_eq!(error.to_string(), r#"expected "alias" at line 1, column 8"#);
}

#[test]
fn a_match_skips_later_candidates() {
    let mut tokens = lexer::from_str("module");
    let mut parser = Parser::new(&mut tokens);
    let rule = choice![keyword("type"), keyword("module"), |_: &mut Parser<
        '_,
        '_,
    >| panic!(
        "attempted a candidate after a match"
    ),];
    assert_eq!(rule.apply(&mut parser).unwrap(), "module");
    assert!(parser.is_eoi().unwrap());
}

#[test]
fn a_consuming_failure_returns_only_the_expectation_at_the_failure() {
    let module = |parser: &mut Parser<'_, '_>| {
        keyword("module").apply(parser)?;
        Err(ParseError::expected("a name", parser.cursor()?))
    };
    let mut tokens = lexer::from_str("module");
    let mut parser = Parser::new(&mut tokens);

    let error = choice![keyword("type"), module, keyword("alias")]
        .apply(&mut parser)
        .unwrap_err();
    assert_eq!(error.to_string(), "expected a name at line 1, column 7");
}

#[test]
fn a_lexical_error_without_consumption_stops_the_choice() {
    let mut tokens = lexer::from_str("\"unfinished");
    let mut parser = Parser::new(&mut tokens);
    let later = |_: &mut Parser<'_, '_>| panic!("attempted a candidate after a lexical error");
    let error = choice![keyword("type"), later]
        .apply(&mut parser)
        .unwrap_err();
    assert!(matches!(
        error,
        ParseError::Lexical(LexError::UnterminatedString { .. })
    ));
}
