use super::{Expr, binding, expression};
use crate::parser::{choice, defer, identifier, list, parse, sequence, symbol};
use aition_core::{
    lexer::{self, Symbol},
    syntax::{ParseError, TriviaKind},
};
use std::{cell::Cell, io::BufReader, rc::Rc};

#[test]
fn a_successful_array_is_retried_as_a_lambda_when_the_binding_suffix_fails() {
    let source = "value = [x] (x);";
    let (result, _) = parse(binding(), lexer::from_str(source)).unwrap();
    assert_eq!(
        result,
        (
            "value".into(),
            Expr::Lambda(vec!["x".into()], Box::new(Expr::Name("x".into())))
        )
    );

    let (result, _) = parse(binding(), lexer::from_str("value = [x];")).unwrap();
    assert_eq!(result.1, Expr::Array(vec![Expr::Name("x".into())]));
}

#[test]
fn eoi_is_a_continuation_that_can_reject_a_shorter_success() {
    let parser = choice![
        symbol(Symbol::OpenParen).map(|()| "short"),
        sequence![symbol(Symbol::OpenParen), symbol(Symbol::CloseParen)].map(|_| "long"),
    ];
    assert_eq!(parse(parser, lexer::from_str("()")).unwrap().0, "long");
}

#[test]
fn a_partially_consumed_failure_can_try_another_candidate() {
    let parser = choice![
        sequence![symbol(Symbol::OpenParen), symbol(Symbol::CloseParen)].map(|_| "first"),
        sequence![symbol(Symbol::OpenParen), symbol(Symbol::CloseBracket)].map(|_| "second"),
    ];
    assert_eq!(parse(parser, lexer::from_str("(]")).unwrap().0, "second");
}

#[test]
fn a_late_failure_can_retry_choices_in_both_halves_of_a_sequence() {
    let prefix = choice![
        symbol(Symbol::OpenParen).map(|()| "short"),
        sequence![symbol(Symbol::OpenParen), symbol(Symbol::OpenBracket)].map(|_| "long"),
    ];
    let middle = choice![
        symbol(Symbol::OpenBracket).map(|()| "left"),
        symbol(Symbol::CloseBracket).map(|()| "right"),
    ];
    let parser = sequence![prefix, middle, symbol(Symbol::CloseParen)];
    assert_eq!(
        parse(parser, lexer::from_str("([])")).unwrap().0,
        ("long", "right", ())
    );
}

#[test]
fn the_first_complete_match_wins_without_evaluating_later_candidates() {
    let tried_second = Rc::new(Cell::new(false));
    let observed = Rc::clone(&tried_second);
    let parser = choice![
        identifier().map(|_| "first"),
        defer(move || {
            observed.set(true);
            identifier().map(|_| "second")
        }),
    ];
    assert_eq!(parse(parser, lexer::from_str("x")).unwrap().0, "first");
    assert!(!tried_second.get());
}

#[test]
fn nested_arrays_and_lambdas_use_the_same_recursive_expression_parser() {
    let source = "[[x] (x), ([y]),]";
    let expected = Expr::Array(vec![
        Expr::Lambda(vec!["x".into()], Box::new(Expr::Name("x".into()))),
        Expr::Array(vec![Expr::Name("y".into())]),
    ]);
    let borrowed = parse(expression(), lexer::from_str(source)).unwrap();
    let streamed = parse(
        expression(),
        lexer::from_reader(BufReader::with_capacity(
            1,
            std::io::Cursor::new(source.as_bytes().to_vec()),
        )),
    )
    .unwrap();
    assert_eq!(borrowed.0, expected);
    assert_eq!(streamed, borrowed);
}

#[test]
fn recursive_lists_allow_empty_and_trailing_commas_but_not_missing_items() {
    let parser = list(identifier(), Symbol::OpenBracket, Symbol::CloseBracket);
    for (source, expected) in [
        ("[]", vec![]),
        ("[x]", vec!["x"]),
        ("[x,]", vec!["x"]),
        ("[x,y]", vec!["x", "y"]),
        ("[x,y,]", vec!["x", "y"]),
    ] {
        assert_eq!(
            parse(parser.clone(), lexer::from_str(source)).unwrap().0,
            expected,
            "{source}"
        );
    }
    for (source, offset) in [("[,]", 1), ("[x,,]", 3), ("[x y]", 3), ("[x,", 3)] {
        let error = parse(parser.clone(), lexer::from_str(source)).unwrap_err();
        assert_eq!(error.cursor().offset, offset, "{source}");
    }
}

#[test]
fn failed_search_reports_the_farthest_expectation_instead_of_the_last_candidate() {
    let source = "value = [x] (x;";
    let error = parse(binding(), lexer::from_str(source)).unwrap_err();
    assert!(matches!(error, ParseError::Unexpected { expected, cursor }
        if expected == "')'" && cursor.offset == source.find(';').unwrap()));
}

#[test]
fn equal_failure_positions_merge_expectations_in_candidate_order() {
    let parser = choice![
        symbol(Symbol::OpenBracket),
        symbol(Symbol::OpenParen),
        symbol(Symbol::OpenBracket),
    ];
    let error = parse(parser, lexer::from_str(" // end\r\n")).unwrap_err();
    assert!(matches!(error, ParseError::Unexpected { expected, cursor }
        if expected == "'[' or '('" && (cursor.line, cursor.column) == (2, 1)));
}

#[test]
fn retries_do_not_relex_input_duplicate_trivia_or_read_after_eoi() {
    let source = "// 幅😀\r\nvalue = [x] // middle\r\n(x); // end";
    let expected_lexemes = lexer::from_str(source).count();
    let mut lexemes = lexer::from_str(source);
    let reads = Rc::new(Cell::new(0));
    let observed = Rc::clone(&reads);
    let mut ended = false;
    let lexemes = std::iter::from_fn(move || {
        assert!(!ended, "must not read after EOI");
        observed.set(observed.get() + 1);
        let next = lexemes.next();
        ended = next.is_none();
        next
    });
    let ((_, value), trivia) = parse(binding(), lexemes).unwrap();
    assert!(matches!(value, Expr::Lambda(_, _)));
    assert_eq!(reads.get(), expected_lexemes + 1);
    let comments: Vec<_> = trivia
        .iter()
        .filter_map(|item| match &item.kind {
            TriviaKind::LineComment(text) => Some(text.as_ref()),
            _ => None,
        })
        .collect();
    assert_eq!(comments, [" 幅😀", " middle", " end"]);
    assert!(
        trivia
            .windows(2)
            .all(|pair| pair[0].end.offset <= pair[1].start.offset)
    );
}

#[test]
fn lexical_errors_abort_alternatives_even_when_found_by_a_continuation() {
    let tried_second = Rc::new(Cell::new(false));
    let observed = Rc::clone(&tried_second);
    let parser = choice![
        identifier(),
        defer(move || {
            observed.set(true);
            identifier()
        }),
    ];
    let error = parse(parser, lexer::from_str("x \"unfinished")).unwrap_err();
    assert!(matches!(error, ParseError::Lexical(_)));
    assert!(!tried_second.get());

    let error = parse(
        expression(),
        lexer::from_reader(std::io::Cursor::new(vec![b'[', b'x', b']', 0xff])),
    )
    .unwrap_err();
    assert!(matches!(error, ParseError::Lexical(_)));
    assert_eq!(error.cursor().offset, 3);
}
