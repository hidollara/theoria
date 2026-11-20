use super::rule::{Rule, repeated, sequence};

pub(super) fn delimited<A: 'static, T: 'static, B: 'static>(
    open: Rule<A>,
    body: Rule<T>,
    close: Rule<B>,
) -> Rule<T> {
    sequence![open, body, close].map(|(_, body, _)| body)
}

/// Read one or more items with a separator between adjacent items.
pub(super) fn separated<T: 'static, S: 'static>(item: Rule<T>, separator: Rule<S>) -> Rule<Vec<T>> {
    let rest = repeated(sequence![separator, item.clone()].map(|(_, value)| value));
    sequence![item, rest].map(|(first, rest)| std::iter::once(first).chain(rest).collect())
}

#[cfg(test)]
mod tests {
    use super::{delimited, separated};
    use crate::input::StrInput;
    use crate::ito::Cursor;
    use crate::ito::lexer::{Lexer, Symbol};
    use crate::ito::parser::ParseError;
    use crate::ito::parser::rule::{symbol, word};

    #[test]
    fn between_requires_an_item_after_the_separator() {
        use Symbol::{CloseBracket, OpenBracket, Pipe};

        let words = delimited(
            symbol(OpenBracket),
            separated(word(), symbol(Pipe)),
            symbol(CloseBracket),
        );
        let mut input = StrInput::new("[a|b]");
        let items = words.apply(&mut Lexer::new(&mut input)).unwrap();
        assert_eq!(
            items,
            [
                ("a".into(), Cursor::default().advanced(['['])),
                ("b".into(), Cursor::default().advanced("[a|".chars()))
            ]
        );

        let mut input = StrInput::new("[a|]");
        let error = words.apply(&mut Lexer::new(&mut input)).unwrap_err();
        let ParseError::Unexpected { expected, cursor } = error else {
            panic!("expected a missing word, got {error:?}");
        };
        assert_eq!(
            (expected, cursor),
            ("a word".into(), Cursor::default().advanced("[a|".chars()))
        );
    }
}
