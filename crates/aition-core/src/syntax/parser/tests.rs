use crate::{
    lexer::{self, Symbol},
    syntax::types::Type,
};

use super::{Expected, Parsable, ParseError, Parser, parse, token::TokenStream};

#[test]
fn eof_diagnostics_need_only_lexemes_after_the_source_and_lexer_are_dropped() {
    for (source, offset, line, column, expected) in [
        ("", 0, 1, 1, Expected::Type),
        ("A.", 2, 1, 3, Expected::Identifier),
        ("A.//幅\r\n\t", 10, 2, 2, Expected::Identifier),
    ] {
        let lexemes = {
            let source = source.to_owned();
            lexer::from_reader(source.as_bytes())
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };

        // Parsing receives an ordinary vector iterator, with no lexer state.
        let error = parse::<Type>(TokenStream::new(lexemes.into_iter().map(Ok)))
            .err()
            .unwrap();
        let ParseError::Unexpected {
            span,
            cursor,
            expected: actual,
        } = error
        else {
            panic!("expected an EOF syntax error, got {error:?}");
        };
        assert_eq!(actual, expected);
        assert_eq!(span, offset..offset);
        assert_eq!(
            (cursor.offset, cursor.line, cursor.column),
            (offset, line, column)
        );
    }
}

#[test]
fn type_parser_leaves_enclosing_delimiters_and_shares_trivia() {
    let mut tokens = TokenStream::new(lexer::from_str("(T<A> | B // first\n), C* // last"));
    assert!(tokens.eat(Symbol::OpenParen).unwrap());
    let (first, mut tokens) = Type::parser().read(tokens).unwrap();
    assert!(matches!(first.unwrap(), Type::Union(_)));
    assert!(tokens.eat(Symbol::CloseParen).unwrap());
    assert!(tokens.eat(Symbol::Comma).unwrap());
    let (second, tokens) = Type::parser().read(tokens).unwrap();
    assert!(matches!(second.unwrap(), Type::Array(_)));
    let trivia = tokens.finish().unwrap();
    let comments: Vec<_> = trivia
        .iter()
        .filter_map(|item| match &item.kind {
            super::TriviaKind::LineComment(text) => Some(text.as_ref()),
            _ => None,
        })
        .collect();
    assert_eq!(comments, [" first", " last"]);
}

#[test]
fn incomplete_type_is_not_mistaken_for_a_boundary() {
    let error = parse::<Type>(TokenStream::new(lexer::from_str("A | ;")))
        .err()
        .unwrap();
    let ParseError::Unexpected { span, expected, .. } = error else {
        panic!("expected an incomplete type error");
    };
    assert_eq!(span, 4..5);
    assert_eq!(expected, Expected::Type);
}
