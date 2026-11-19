use super::super::{ParseError, Rule, choice, repeated, symbol, trivia};
use super::rule;
use crate::ito::lexer::{self, LexError, Symbol, TokenKind, TokenStreamItem};
use crate::ito::parser::Parser;
use crate::ito::parser::tests::ReadFragment;

#[test]
fn initial_misses_preserve_input_but_partial_matches_report_the_missing_part() {
    let rule = rule("'('", |parser| {
        parser.read(symbol(Symbol::OpenParen))?;
        parser.read(symbol(Symbol::CloseParen))
    });
    let mut stream = lexer::from_str("[");
    let (value, mut parser) = rule.read_fragment(&mut stream).unwrap();
    assert_eq!(value, None);
    assert_eq!(parser.consumed(), 0);
    assert_eq!(
        parser.next_if(|_| true).unwrap().unwrap().kind,
        TokenKind::Symbol(Symbol::OpenBracket)
    );
    assert!(matches!(parser.peek().unwrap(), TokenStreamItem::Eoi(_)));

    let error = rule
        .read_fragment(&mut lexer::from_str("(]"))
        .err()
        .unwrap();
    assert!(matches!(error, ParseError::Unexpected { expected, cursor }
        if expected == "')'" && cursor.offset == 1));
}

#[test]
fn empty_matches_do_not_commit_or_change_the_callers_boundary() {
    let rule = rule("dots and a comma", |parser| {
        let dots = parser.read(repeated(symbol(Symbol::Dot)))?;
        parser.read(symbol(Symbol::Comma))?;
        Ok(dots)
    });
    let mut stream = lexer::from_str(";");
    let (value, mut parser) = rule.read_fragment(&mut stream).unwrap();
    assert_eq!(value, None);
    assert_eq!(parser.consumed(), 0);
    assert_eq!(
        parser.next_if(|_| true).unwrap().unwrap().kind,
        TokenKind::Symbol(Symbol::Semicolon)
    );

    let mut stream = lexer::from_str(" ,");
    let mut parser = Parser::new(&mut stream);
    assert!(rule.read(&mut parser).unwrap().is_none());
    parser.read(repeated(trivia())).unwrap();
    assert_eq!(rule.read(&mut parser).unwrap(), Some(vec![]));
    assert!(matches!(parser.peek().unwrap(), TokenStreamItem::Eoi(_)));
}

#[test]
fn closures_build_results_directly_and_can_capture_configuration() {
    let prefix = "parsed".to_owned();
    let group = rule("'('", move |parser| {
        parser.read(symbol(Symbol::OpenParen))?;
        parser.read(repeated(trivia()))?;
        let dots = parser.read(repeated(rule("'.'", |parser| {
            parser.read(symbol(Symbol::Dot))?;
            parser.read(repeated(trivia()))
        })))?;
        parser.read(symbol(Symbol::CloseParen))?;
        Ok(format!("{prefix}: {}", dots.len()))
    });
    for (source, expected) in [("(..)", "parsed: 2"), ("( // gap\n . )", "parsed: 1")] {
        let mut stream = lexer::from_str(source);
        let (value, mut parser) = group.read_fragment(&mut stream).unwrap();
        assert_eq!(value.as_deref(), Some(expected));
        assert!(matches!(parser.peek().unwrap(), TokenStreamItem::Eoi(_)));
    }
}

#[test]
fn adjacency_applies_only_to_the_requested_boundary() {
    let rule = rule("'<'", |parser| {
        parser.read(symbol(Symbol::OpenAngle))?;
        parser.read(symbol(Symbol::Dot))?;
        parser.read(repeated(trivia()))?;
        parser.read(symbol(Symbol::Slash))?;
        parser.read(symbol(Symbol::CloseAngle))
    });
    for source in ["<./>", "<. />", "<. // note\n/>"] {
        let mut stream = lexer::from_str(source);
        let (value, mut parser) = rule.read_fragment(&mut stream).unwrap();
        assert!(value.is_some());
        assert!(matches!(parser.peek().unwrap(), TokenStreamItem::Eoi(_)));
    }
    for (source, offset) in [
        ("< ./>", 1),
        ("<// note\n./>", 1),
        ("<./ >", 3),
        ("<. / // note\n>", 4),
    ] {
        let error = rule
            .read_fragment(&mut lexer::from_str(source))
            .err()
            .unwrap();
        assert_eq!(error.cursor().offset, offset, "{source:?}");
    }
}

#[test]
fn nested_closures_inherit_the_first_boundary_and_allow_their_own_internal_gaps() {
    let rule = rule("'<'", |parser| {
        parser.read(symbol(Symbol::OpenAngle))?;
        parser.read(rule("'.'", |parser| {
            parser.read(symbol(Symbol::Dot))?;
            parser.read(repeated(trivia()))?;
            parser.read(symbol(Symbol::Comma))
        }))?;
        parser.read(repeated(trivia()))?;
        parser.read(symbol(Symbol::CloseAngle))
    });
    let mut stream = lexer::from_str("<. , >");
    let (value, mut parser) = rule.read_fragment(&mut stream).unwrap();
    assert!(value.is_some());
    assert!(matches!(parser.peek().unwrap(), TokenStreamItem::Eoi(_)));
    let error = rule
        .read_fragment(&mut lexer::from_str("< .,>"))
        .err()
        .unwrap();
    assert_eq!(error.cursor().offset, 1);
}

#[test]
fn closing_parenthesis_requires_adjacency_after_an_optional_dot() {
    let rule = rule("'('", |parser| {
        parser.read(symbol(Symbol::OpenParen))?;
        let gap = parser.read(repeated(trivia()))?;
        if symbol(Symbol::Dot).read(parser)?.is_none()
            && let Some(first) = gap.first()
        {
            return Err(ParseError::expected("')'".into(), first.start));
        }
        parser.read(symbol(Symbol::CloseParen))
    });
    for source in ["()", "(.)", "( .)"] {
        let mut stream = lexer::from_str(source);
        let (value, mut parser) = rule.read_fragment(&mut stream).unwrap();
        assert!(value.is_some());
        assert!(matches!(parser.peek().unwrap(), TokenStreamItem::Eoi(_)));
    }
    for (source, offset) in [("( )", 1), ("(. )", 2), ("(// note\n)", 1)] {
        let error = rule
            .read_fragment(&mut lexer::from_str(source))
            .err()
            .unwrap();
        assert_eq!(error.cursor().offset, offset);
    }
}

#[test]
fn lexical_and_input_errors_are_fatal_even_before_consumption() {
    let rule = rule("'.'", |parser| parser.read(symbol(Symbol::Dot)));
    let error = rule
        .read_fragment(&mut lexer::from_str("\"unfinished"))
        .err()
        .unwrap();
    assert!(matches!(
        error,
        ParseError::Lexical(LexError::UnterminatedString { .. })
    ));
    let error = rule
        .read_fragment(&mut lexer::from_reader(&b"\xff"[..]))
        .err()
        .unwrap();
    assert!(matches!(error, ParseError::Lexical(LexError::Input(_))));
}

#[test]
fn nested_misses_are_recoverable_only_before_consumption() {
    let first = rule("'('", |parser| {
        parser.read(rule("'('", |parser| {
            parser.read(symbol(Symbol::OpenParen))?;
            parser.read(symbol(Symbol::CloseParen))
        }))
    });
    let rule = choice![first, symbol(Symbol::Comma)];
    let mut stream = lexer::from_str(",");
    let (value, mut parser) = rule.read_fragment(&mut stream).unwrap();
    assert!(value.is_some());
    assert!(matches!(parser.peek().unwrap(), TokenStreamItem::Eoi(_)));
    let error = rule
        .read_fragment(&mut lexer::from_str("(,"))
        .err()
        .unwrap();
    assert!(matches!(error, ParseError::Unexpected { expected, cursor }
        if expected == "')'" && cursor.offset == 1));
}

#[test]
fn rules_return_none_only_for_an_unconsumed_mismatch() {
    let rule = rule("'['", |parser| {
        parser.read(symbol(Symbol::OpenBracket))?;
        parser.read(repeated(trivia()))?;
        let present = rule("'('", |parser| {
            parser.read(symbol(Symbol::OpenParen))?;
            parser.read(symbol(Symbol::CloseParen))
        })
        .read(parser)?;
        parser.read(repeated(trivia()))?;
        parser.read(symbol(Symbol::CloseBracket))?;
        Ok(present)
    });
    for (source, expected) in [
        ("[]", None),
        ("[ // absent\n ]", None),
        ("[ // present\n () ]", Some(())),
    ] {
        let mut stream = lexer::from_str(source);
        let (value, mut parser) = rule.read_fragment(&mut stream).unwrap();
        assert_eq!(value.map(|inner| inner.map(|_| ())), Some(expected));
        assert!(matches!(parser.peek().unwrap(), TokenStreamItem::Eoi(_)));
    }
    let error = rule
        .read_fragment(&mut lexer::from_str("[(]"))
        .err()
        .unwrap();
    assert!(matches!(error, ParseError::Unexpected { expected, cursor }
        if expected == "')'" && cursor.offset == 2));
    let error = rule
        .read_fragment(&mut lexer::from_str("[\"unfinished"))
        .err()
        .unwrap();
    assert!(matches!(
        error,
        ParseError::Lexical(LexError::UnterminatedString { .. })
    ));
    let error = rule
        .read_fragment(&mut lexer::from_reader(&b"[\xff"[..]))
        .err()
        .unwrap();
    assert!(matches!(error, ParseError::Lexical(LexError::Input(_))));
}
