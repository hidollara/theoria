use super::Rule;
use super::error::ParseError;
use super::state::Parser;

/// Read `R = (item R) or ε` iteratively, with one consuming item per step.
pub(super) fn repeated<R: Rule>(rule: R) -> impl Rule<Output = Vec<R::Output>> {
    move |parser: &mut Parser<'_, '_>| {
        let mut values = Vec::new();
        loop {
            let consumed = parser.consumed();
            let value = match rule.apply(parser) {
                Ok(value) => value,
                Err(ParseError::Unexpected { .. }) if parser.consumed() == consumed => {
                    return Ok(values);
                }
                Err(error) => return Err(error),
            };
            assert!(
                parser.consumed() > consumed,
                "a repeated rule must consume a token"
            );
            values.push(value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::repeated;
    use crate::ito::lexer::{self, LexError, Symbol};
    use crate::ito::parser::Rule;
    use crate::ito::parser::error::ParseError;
    use crate::ito::parser::state::Parser;
    use crate::ito::parser::trivia::trivia;

    #[test]
    fn no_matching_item_returns_an_empty_list_without_advancing() {
        let mut stream = lexer::from_str("word");
        let mut parser = Parser::new(&mut stream);

        let items = repeated(trivia).apply(&mut parser).unwrap();

        assert!(items.is_empty());
        assert_eq!(parser.cursor().unwrap().offset, 0);
    }

    #[test]
    fn stops_before_the_first_nonmatching_token() {
        let prefix = "// comment\n";
        let text = format!("{prefix}word");
        let mut stream = lexer::from_str(&text);
        let mut parser = Parser::new(&mut stream);

        let items = repeated(trivia).apply(&mut parser).unwrap();

        assert_eq!(items.len(), 2);
        assert_eq!(parser.cursor().unwrap().offset, prefix.len());
    }

    #[test]
    fn missing_closing_paren_in_the_second_pair_is_an_error() {
        fn expect_symbol(parser: &mut Parser<'_, '_>, symbol: Symbol) -> Result<(), ParseError> {
            match parser.symbol(symbol)? {
                Some(_) => Ok(()),
                None => Err(ParseError::expected(symbol, parser.cursor()?)),
            }
        }

        let paren_pair = |parser: &mut Parser<'_, '_>| {
            expect_symbol(parser, Symbol::OpenParen)?;
            expect_symbol(parser, Symbol::CloseParen)
        };

        let text = "()(";
        let mut stream = lexer::from_str(text);
        let mut parser = Parser::new(&mut stream);

        let error = repeated(paren_pair).apply(&mut parser).unwrap_err();

        assert!(matches!(error, ParseError::Unexpected { expected, cursor }
            if expected == ")" && cursor.offset == text.len()));
    }

    #[test]
    fn lexical_error_after_a_matched_space_is_reported() {
        let mut stream = lexer::from_str(" \"unfinished");
        let mut parser = Parser::new(&mut stream);

        let error = repeated(trivia).apply(&mut parser).unwrap_err();

        assert!(matches!(
            error,
            ParseError::Lexical(LexError::UnterminatedString { .. })
        ));
        assert_eq!(error.cursor().offset, 1);
    }

    #[test]
    fn invalid_utf8_before_the_first_item_is_an_error() {
        let mut stream = lexer::from_reader(&b"\xff"[..]);
        let mut parser = Parser::new(&mut stream);

        let error = repeated(trivia).apply(&mut parser).unwrap_err();

        assert!(matches!(error, ParseError::Lexical(LexError::Input(_))));
    }
}
