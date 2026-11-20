use super::{Rule, optional};

/// Read a delimited list, allowing a separator immediately before the closing delimiter.
pub fn list<A: 'static, T: 'static, S: 'static, B: 'static>(
    open: Rule<A>,
    item: Rule<T>,
    separator: Rule<S>,
    close: Rule<B>,
) -> Rule<Vec<T>> {
    let optional_close = optional(close.clone());
    let separator = optional(separator);
    Rule::new(move |tokens| {
        open.apply(tokens)?;
        let mut items = Vec::new();
        while optional_close.apply(tokens)?.is_none() {
            let start = tokens.cursor();
            items.push(item.apply(tokens)?);
            if separator.apply(tokens)?.is_none() {
                close.apply(tokens)?;
                break;
            }
            assert!(tokens.cursor() != start, "a list step must consume a token");
        }
        Ok(items)
    })
}

#[cfg(test)]
mod tests {
    use super::list;
    use crate::input::StrInput;
    use crate::ito::Cursor;
    use crate::ito::lexer::{Lexer, Symbol};
    use crate::ito::parser::ParseError;
    use crate::ito::parser::rule::{sequence, symbol, word};

    #[test]
    fn pipe_can_separate_items_and_trail_the_last_one() {
        use Symbol::{CloseBracket, OpenBracket, Pipe};

        let words = list(
            symbol(OpenBracket),
            word(),
            symbol(Pipe),
            symbol(CloseBracket),
        );
        let mut input = StrInput::new("[a|b|]");
        let items = words.apply(&mut Lexer::new(&mut input)).unwrap();
        assert_eq!(
            items,
            [
                ("a".into(), Cursor::default().advanced(['['])),
                ("b".into(), Cursor::default().advanced("[a|".chars()))
            ]
        );
    }

    #[test]
    fn a_trailing_separator_must_match_the_whole_rule() {
        use Symbol::{CloseBracket, OpenBracket, Pipe};

        let separator = sequence![symbol(Pipe), symbol(Pipe)];
        let words = list(symbol(OpenBracket), word(), separator, symbol(CloseBracket));
        let mut input = StrInput::new("[a||]");
        assert_eq!(
            words.apply(&mut Lexer::new(&mut input)).unwrap(),
            [("a".into(), Cursor::default().advanced(['[']))]
        );

        let mut input = StrInput::new("[a|]");
        let error = words.apply(&mut Lexer::new(&mut input)).unwrap_err();
        let ParseError::Unexpected { expected, cursor } = error else {
            panic!("expected a missing separator token, got {error:?}");
        };
        assert_eq!(
            (expected, cursor),
            ("|".into(), Cursor::default().advanced("[a|".chars()))
        );
    }
}
