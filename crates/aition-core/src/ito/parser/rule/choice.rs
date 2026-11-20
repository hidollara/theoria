use super::super::error::ParseError;
use super::Rule;

pub fn choice<T: 'static, const N: usize>(candidates: [Rule<T>; N]) -> Rule<T> {
    Rule::new(move |tokens| {
        let start = tokens.cursor();
        let mut earlier = Vec::new();
        for (index, candidate) in candidates.iter().enumerate() {
            match candidate.apply(tokens) {
                Err(ParseError::Unexpected { expected, cursor }) if tokens.cursor() == start => {
                    if index + 1 == N {
                        let expected = match earlier.as_slice() {
                            [] => expected,
                            [first] => format!("{first} or {expected}"),
                            others => format!("{}, or {expected}", others.join(", ")),
                        };
                        return Err(ParseError::expected(expected, cursor));
                    }
                    earlier.push(expected);
                }
                result => return result,
            }
        }
        Err(ParseError::expected("", tokens.next_start()?))
    })
}

#[cfg(test)]
mod tests {
    use super::choice;
    use crate::input::StrInput;
    use crate::ito::Cursor;
    use crate::ito::lexer::{LexError, Lexer};
    use crate::ito::parser::error::ParseError;
    use crate::ito::parser::rule::{eoi, keyword, sequence, word};

    #[test]
    fn empty_choice_reports_the_next_token_without_consuming_it() {
        let mut input = StrInput::new(" // comment\nname");
        let mut tokens = Lexer::new(&mut input);
        let error = choice::<(), 0>([]).apply(&mut tokens).unwrap_err();
        assert_eq!(error.to_string(), "unexpected input at line 2, column 1");
        assert_eq!(
            word().apply(&mut tokens).unwrap(),
            (
                "name".into(),
                Cursor::default().advanced(" // comment\n".chars())
            )
        );
    }

    #[test]
    fn all_misses_report_the_candidates_in_order_without_consuming_input() {
        let mut input = StrInput::new(" unknown");
        let mut tokens = Lexer::new(&mut input);
        let error = choice([keyword("type")]).apply(&mut tokens).unwrap_err();
        assert_eq!(error.to_string(), r#"expected "type" at line 1, column 2"#);

        let error = choice([keyword("type"), keyword("module")])
            .apply(&mut tokens)
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            r#"expected "type" or "module" at line 1, column 2"#
        );

        let error = choice([keyword("type"), keyword("module"), keyword("alias")])
            .apply(&mut tokens)
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            r#"expected "type", "module", or "alias" at line 1, column 2"#
        );
        assert_eq!(
            word().apply(&mut tokens).unwrap(),
            ("unknown".into(), Cursor::default().advanced([' ']))
        );
    }

    #[test]
    fn a_match_skips_later_candidates() {
        let mut input = StrInput::new("module");
        let mut tokens = Lexer::new(&mut input);
        let rule = choice([
            keyword("type"),
            keyword("module"),
            super::Rule::new(|_| panic!("attempted a candidate after a match")),
        ]);
        assert_eq!(
            rule.apply(&mut tokens).unwrap(),
            crate::ito::Cursor::default()
        );
        eoi().apply(&mut tokens).unwrap();
    }

    #[test]
    fn a_consuming_failure_returns_only_the_expectation_at_the_failure() {
        let module =
            sequence![keyword("module"), keyword("name").expecting("a name")].map(|(_, name)| name);
        let mut input = StrInput::new("module");
        let mut tokens = Lexer::new(&mut input);
        let error = choice([keyword("type"), module, keyword("alias")])
            .apply(&mut tokens)
            .unwrap_err();
        assert_eq!(error.to_string(), "expected a name at line 1, column 7");
    }

    #[test]
    fn a_lexical_error_without_consumption_stops_the_choice() {
        let mut input = StrInput::new("\"unfinished");
        let mut tokens = Lexer::new(&mut input);
        let later = super::Rule::new(|_| panic!("attempted a candidate after a lexical error"));
        let error = choice([keyword("type"), later])
            .apply(&mut tokens)
            .unwrap_err();
        assert!(matches!(
            error,
            ParseError::Lexical(LexError::UnterminatedString { .. })
        ));
    }
}
