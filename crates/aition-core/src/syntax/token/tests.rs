use super::{Lookahead, TokenKind, TokenStream, Trivia, TriviaKind};
use crate::lexer::{self, Cursor, LexemeIterator, Symbol};
use std::borrow::Cow;

#[test]
fn marked_spans_keep_byte_ranges_and_positions_without_lookahead_or_trailing_trivia() {
    let source = " \r\n幅\r\n高 // tail";
    check(lexer::from_str(source));
    check(lexer::from_reader(source.as_bytes()));

    fn check<'source>(lexemes: impl LexemeIterator<'source>) {
        let mut input = TokenStream::new(lexemes);
        let outer = input.mark().unwrap();
        input.next_if(|_| true).unwrap().unwrap();

        // Marking the next line reads ahead but must not extend the consumed range.
        let inner = input.mark().unwrap();
        let span = input.span(&outer);
        assert_eq!(span.range(), 3..6);
        assert_eq!(position(span.start), (2, 1));
        assert_eq!(position(span.end), (2, 2));

        // Nothing has been consumed since the inner mark, so both positions coincide.
        let empty = input.span(&inner);
        assert_eq!(empty.range(), 8..8);
        assert_eq!(empty.start, empty.end);
        assert_eq!(position(empty.end), (3, 1));

        input.next_if(|_| true).unwrap().unwrap();
        assert!(matches!(input.peek().unwrap(), Lookahead::Eoi(_)));
        let span = input.span(&outer);
        assert_eq!(span.range(), 3..11);
        assert_eq!(position(span.start), (2, 1));
        assert_eq!(position(span.end), (3, 2));
        assert_eq!(input.span(&inner).range(), 8..11);
    }

    fn position(cursor: Cursor) -> (usize, usize) {
        (cursor.line, cursor.column)
    }
}

#[test]
fn a_mark_at_eof_has_equal_start_and_end_after_trivia() {
    let source = " // 終\r\n";
    let mut input = TokenStream::new(lexer::from_str(source));
    let mark = input.mark().unwrap();
    let span = input.span(&mark);
    assert_eq!(span.start, span.end);
    assert_eq!(span.range(), source.len()..source.len());
    assert_eq!((span.end.line, span.end.column), (2, 1));
}

#[test]
fn eoi_keeps_its_cursor_and_does_not_read_the_iterator_again() {
    for source in ["", " // 終\r\n", "word // 終\r\n"] {
        check(lexer::from_str(source), source);
        check(lexer::from_reader(source.as_bytes()), source);
    }

    fn check<'s>(mut lexemes: impl LexemeIterator<'s>, source: &str) {
        let mut ended = false;
        let lexemes = std::iter::from_fn(|| {
            assert!(!ended, "the iterator must not be read after EOI");
            let next = lexemes.next();
            ended = next.is_none();
            next
        });
        let mut input = TokenStream::new(lexemes);
        while input.next_if(|_| true).unwrap().is_some() {}
        let trivia_count = input.trivia.len();
        for _ in 0..3 {
            let Lookahead::Eoi(cursor) = input.peek().unwrap() else {
                panic!("expected EOI");
            };
            assert_eq!(cursor.offset, source.len());
            assert_eq!(
                (cursor.line, cursor.column),
                (if source.is_empty() { 1 } else { 2 }, 1)
            );
            let cursor = input.peek().unwrap().cursor();
            assert!(
                input
                    .next_if(|_| panic!("EOI is not a token"))
                    .unwrap()
                    .is_none()
            );
            let mark = input.mark().unwrap();
            let span = input.span(&mark);
            assert_eq!(span.start, cursor);
            assert_eq!(span.end, cursor);
            assert_eq!(input.trivia.len(), trivia_count);
        }
    }
}

#[test]
fn peeking_and_consuming_do_not_interpret_words_or_duplicate_trivia() {
    let mut input = TokenStream::new(lexer::from_str("// note\n123invalid "));
    for _ in 0..3 {
        let Lookahead::Token(token) = input.peek().unwrap() else {
            panic!("expected a word");
        };
        assert_eq!(token.kind, TokenKind::Word("123invalid".into()));
        assert_eq!(token.span().range(), 8..18);
    }
    assert_eq!(
        input.next_if(|_| true).unwrap().unwrap().span().range(),
        8..18
    );
    assert!(input.next_if(|_| true).unwrap().is_none());
    assert!(matches!(input.peek().unwrap(), Lookahead::Eoi(_)));
    let trivia: Vec<_> = input
        .into_trivia()
        .into_iter()
        .map(|item| item.kind)
        .collect();
    assert_eq!(
        trivia,
        [
            TriviaKind::LineComment(" note".into()),
            TriviaKind::Whitespace("\n".into()),
            TriviaKind::Whitespace(" ".into()),
        ]
    );
}

#[test]
fn mismatched_symbols_leave_the_next_token_unconsumed() {
    let mut input = TokenStream::new(lexer::from_str(" ;"));
    assert!(
        input
            .next_if(|kind| matches!(kind, TokenKind::Symbol(Symbol::Comma)))
            .unwrap()
            .is_none()
    );
    let Lookahead::Token(token) = input.peek().unwrap() else {
        panic!("expected a semicolon");
    };
    let span = token.span();
    assert_eq!(span.range(), 1..2);
    assert_eq!((span.start.line, span.start.column), (1, 2));
    assert!(
        input
            .next_if(|kind| matches!(kind, TokenKind::Symbol(Symbol::Semicolon)))
            .unwrap()
            .is_some()
    );
    assert_eq!(collected_trivia(input).len(), 1);
}

#[test]
fn next_if_returns_the_token_and_only_advances_on_a_match() {
    let mut input = TokenStream::new(lexer::from_str(" // note\nword \"text\""));
    let mark = input.mark().unwrap();
    assert!(
        input
            .next_if(|kind| matches!(kind, TokenKind::String(_)))
            .unwrap()
            .is_none()
    );
    assert_eq!(input.span(&mark).range(), 9..9);

    let word = input
        .next_if(|kind| matches!(kind, TokenKind::Word(_)))
        .unwrap()
        .unwrap();
    assert_eq!(word.kind, TokenKind::Word("word".into()));
    assert_eq!(word.span().range(), 9..13);
    assert_eq!(input.span(&mark), word.span());

    let text = input
        .next_if(|kind| matches!(kind, TokenKind::String(_)))
        .unwrap()
        .unwrap();
    assert_eq!(text.kind, TokenKind::String("text".into()));
    assert_eq!(input.span(&mark).range(), 9..20);
    assert!(
        input
            .next_if(|_| panic!("EOF has no token to test"))
            .unwrap()
            .is_none()
    );
    let trivia = collected_trivia(input);
    assert_eq!(trivia.len(), 4);
    assert_eq!(trivia[1].kind, TriviaKind::LineComment(" note".into()));
}

#[test]
fn collecting_trivia_does_not_validate_or_read_the_remainder() {
    let mut input = TokenStream::new(lexer::from_str("// note\nword \"unfinished"));
    assert!(input.next_if(|_| false).unwrap().is_none());
    let trivia = input.into_trivia();
    assert_eq!(trivia.len(), 2);
    assert_eq!(trivia[0].kind, TriviaKind::LineComment(" note".into()));
}

#[test]
fn trivia_keeps_both_cursor_endpoints_and_full_byte_ranges() {
    let source = " \t//幅😀\r\n  word // end";
    check(lexer::from_str(source), source);
    check(
        lexer::from_reader(std::io::BufReader::with_capacity(1, source.as_bytes())),
        source,
    );

    fn check<'s>(lexemes: impl LexemeIterator<'s>, source: &str) {
        let mut input = TokenStream::new(lexemes);
        while input.next_if(|_| true).unwrap().is_some() {}
        let trivia = input.into_trivia();
        let expected = [
            (0..2, (1, 1), (1, 3), " \t"),
            (2..11, (1, 3), (1, 7), "//幅😀"),
            (11..15, (1, 7), (2, 3), "\r\n  "),
            (19..20, (2, 7), (2, 8), " "),
            (20..26, (2, 8), (2, 14), "// end"),
        ];
        assert_eq!(trivia.len(), expected.len());
        for (item, (range, start, end, text)) in trivia.iter().zip(expected) {
            let span = item.span();
            assert_eq!(span.start, item.start);
            assert_eq!(span.end, item.end);
            assert_eq!(span.range(), range);
            assert_eq!((item.start.line, item.start.column), start);
            assert_eq!((item.end.line, item.end.column), end);
            assert_eq!(&source[span.range()], text);
        }
    }
}

#[test]
fn grammar_tokens_preserve_text_positions_and_ownership() {
    let source = "// c\nword \"x//y\" ; // end";
    check(lexer::from_str(source), true);
    check(lexer::from_reader(source.as_bytes()), false);

    fn check<'source>(lexemes: impl LexemeIterator<'source>, borrowed: bool) {
        let mut input = TokenStream::new(lexemes);
        for (expected_kind, expected_text, expected_span) in [
            ("word", "word", 5..9),
            ("string", "x//y", 10..16),
            ("symbol", ";", 17..18),
        ] {
            let token = input.next_if(|_| true).unwrap().unwrap();
            assert_eq!(token.span().range(), expected_span);
            // Exhaustive: trivia is not representable in a grammar token.
            let (kind, text) = match token.kind {
                TokenKind::Word(text) => ("word", text),
                TokenKind::String(text) => ("string", text),
                TokenKind::Symbol(symbol) => ("symbol", Cow::Owned(symbol.to_string())),
            };
            assert_eq!((kind, text.as_ref()), (expected_kind, expected_text));
            if kind != "symbol" {
                assert_eq!(matches!(text, Cow::Borrowed(_)), borrowed);
            }
        }
        let trivia = collected_trivia(input);
        let comments: Vec<_> = trivia
            .iter()
            .filter_map(|item| match &item.kind {
                TriviaKind::LineComment(text) => Some(text.as_ref()),
                _ => None,
            })
            .collect();
        assert_eq!(comments, [" c", " end"]);
    }
}

fn collected_trivia<'s>(mut input: TokenStream<'s, impl LexemeIterator<'s>>) -> Vec<Trivia> {
    assert!(input.next_if(|_| true).unwrap().is_none());
    input.into_trivia()
}
