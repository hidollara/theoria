use super::super::error::ParseError;
use super::Rule;

/// An unconsumed mismatch means the optional part is absent.
pub fn optional<T: 'static>(rule: Rule<T>) -> Rule<Option<T>> {
    Rule::new(move |tokens| {
        let start = tokens.cursor();
        match rule.apply(tokens) {
            Ok(value) => Ok(Some(value)),
            Err(ParseError::Unexpected { .. }) if tokens.cursor() == start => Ok(None),
            Err(error) => Err(error),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::optional;
    use crate::input::StrInput;
    use crate::ito::lexer::{Lexer, Symbol};
    use crate::ito::parser::error::ParseError;
    use crate::ito::parser::rule::{eoi, sequence, symbol};

    #[test]
    fn eoi_can_match_without_consuming_a_token() {
        let mut input = StrInput::new("");
        let mut tokens = Lexer::new(&mut input);
        assert_eq!(optional(eoi()).apply(&mut tokens).unwrap(), Some(()));
        assert_eq!(tokens.cursor().offset, 0);
    }

    #[test]
    fn a_nested_mismatch_is_relative_to_each_optional_rule() {
        let pair = optional(sequence![
            symbol(Symbol::OpenParen),
            optional(symbol(Symbol::Comma)),
            symbol(Symbol::CloseParen)
        ]);
        let mut input = StrInput::new(")(");
        let mut tokens = Lexer::new(&mut input);
        assert_eq!(pair.apply(&mut tokens).unwrap(), None);
        symbol(Symbol::CloseParen).apply(&mut tokens).unwrap();
        let error = pair.apply(&mut tokens).unwrap_err();
        assert!(matches!(error, ParseError::Unexpected { expected, cursor }
                    if expected == ")" && cursor.offset == 2));
    }
}
