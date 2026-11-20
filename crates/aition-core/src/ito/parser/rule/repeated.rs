use super::{Rule, optional};

/// Read `R = (item R) or ε` iteratively, with one consuming item per step.
pub fn repeated<T: 'static>(rule: Rule<T>) -> Rule<Vec<T>> {
    let next = optional(rule);
    Rule::new(move |tokens| {
        let mut values = Vec::new();
        loop {
            let start = tokens.cursor();
            let Some(value) = next.apply(tokens)? else {
                return Ok(values);
            };
            assert!(
                tokens.cursor() != start,
                "a repeated rule must consume a token"
            );
            values.push(value);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::repeated;
    use crate::input::{ReaderInput, StrInput};
    use crate::ito::Cursor;
    use crate::ito::lexer::{LexError, Lexer, Symbol};
    use crate::ito::parser::error::ParseError;
    use crate::ito::parser::rule::{eoi, sequence, symbol, word};

    #[test]
    fn no_matching_item_returns_an_empty_list_without_advancing() {
        let mut input = StrInput::new("word");
        let mut tokens = Lexer::new(&mut input);
        let items = repeated(symbol(Symbol::Comma)).apply(&mut tokens).unwrap();

        assert_eq!(items, []);
        assert_eq!(
            word().apply(&mut tokens).unwrap(),
            ("word".into(), Cursor::default())
        );
    }

    #[test]
    #[should_panic(expected = "a repeated rule must consume a token")]
    fn repeating_eoi_cannot_make_progress() {
        let mut input = StrInput::new("");
        let mut tokens = Lexer::new(&mut input);
        repeated(eoi()).apply(&mut tokens).unwrap();
    }

    #[test]
    fn stops_before_the_first_nonmatching_token() {
        let mut input = StrInput::new(",,word");
        let mut tokens = Lexer::new(&mut input);
        let items = repeated(symbol(Symbol::Comma)).apply(&mut tokens).unwrap();

        assert_eq!(
            items,
            [Cursor::default(), Cursor::default().advanced([','])]
        );
        assert_eq!(
            word().apply(&mut tokens).unwrap(),
            ("word".into(), Cursor::default().advanced(",,".chars()))
        );
    }

    #[test]
    fn missing_closing_paren_in_the_second_pair_is_an_error() {
        let paren_pair = sequence![symbol(Symbol::OpenParen), symbol(Symbol::CloseParen)];

        let text = "()(";
        let mut input = StrInput::new(text);
        let mut tokens = Lexer::new(&mut input);
        let error = repeated(paren_pair).apply(&mut tokens).unwrap_err();

        assert!(matches!(error, ParseError::Unexpected { expected, cursor }
                    if expected == ")" && cursor.offset == text.len()));
    }

    #[test]
    fn lexical_error_after_a_matched_item_is_reported() {
        let mut input = StrInput::new(",\"unfinished");
        let mut tokens = Lexer::new(&mut input);
        let error = repeated(symbol(Symbol::Comma))
            .apply(&mut tokens)
            .unwrap_err();

        assert!(matches!(
            error,
            ParseError::Lexical(LexError::UnterminatedString { .. })
        ));
        assert_eq!(error.cursor().offset, 1);
    }

    #[test]
    fn invalid_utf8_before_the_first_item_is_an_error() {
        let mut input = ReaderInput::new(&b"\xff"[..]);
        let mut tokens = Lexer::new(&mut input);
        let error = repeated(symbol(Symbol::Comma))
            .apply(&mut tokens)
            .unwrap_err();

        assert!(matches!(error, ParseError::Lexical(LexError::Input { .. })));
    }
}
