//! Token-level rules and their combinators.

mod choice;
mod list;
mod optional;
mod repeated;
pub(super) mod sequence;

use super::error::ParseError;
use crate::ito::Cursor;
use crate::ito::lexer::{Lexer, Symbol, Token};
use std::fmt;
use std::rc::{Rc, Weak};

pub(super) use self::choice::choice;
pub(super) use self::list::list;
pub(super) use self::optional::optional;
pub(super) use self::repeated::repeated;
pub(super) use self::sequence::sequence;

pub(super) struct Rule<T>(Rc<dyn Fn(&mut Lexer<'_, '_>) -> Result<T, ParseError>>);

impl<T> Clone for Rule<T> {
    fn clone(&self) -> Self {
        Self(Rc::clone(&self.0))
    }
}

/// Build the body once, passing a rule that refers back to it.
pub(super) fn recursive<T: 'static>(build: impl FnOnce(Rule<T>) -> Rule<T>) -> Rule<T> {
    let body = Rc::new_cyclic(|weak: &Weak<Rule<T>>| {
        let weak = Weak::clone(weak);
        let reference = Rule::new(move |tokens| {
            let body = weak.upgrade().expect("recursive rule requires its owner");
            body.apply(tokens)
        });
        build(reference)
    });
    Rule::new(move |tokens| body.apply(tokens))
}

impl<T: 'static> Rule<T> {
    fn new(read: impl Fn(&mut Lexer<'_, '_>) -> Result<T, ParseError> + 'static) -> Self {
        Self(Rc::new(read))
    }

    pub(super) fn apply(&self, tokens: &mut Lexer<'_, '_>) -> Result<T, ParseError> {
        (self.0)(tokens)
    }

    pub(super) fn map<U: 'static>(self, convert: impl Fn(T) -> U + 'static) -> Rule<U> {
        Rule::new(move |tokens| self.apply(tokens).map(&convert))
    }

    pub(super) fn try_map<U: 'static>(
        self,
        convert: impl Fn(T) -> Result<U, ParseError> + 'static,
    ) -> Rule<U> {
        Rule::new(move |tokens| self.apply(tokens).and_then(&convert))
    }

    pub(super) fn expecting(self, description: impl fmt::Display + 'static) -> Rule<T> {
        Rule::new(move |tokens| self.apply(tokens).map_err(|e| e.expecting(&description)))
    }
}

pub(super) fn eoi() -> Rule<()> {
    Rule::new(|tokens| {
        if let Some(token) = tokens.peek()? {
            return Err(ParseError::expected("end of input", token.cursor()));
        }
        Ok(())
    })
}

/// Match a boundary with no intervening trivia, without consuming the next token.
pub(super) fn adjacent() -> Rule<()> {
    Rule::new(|tokens| {
        if tokens.cursor() == tokens.next_start()? {
            Ok(())
        } else {
            Err(ParseError::expected("an adjacent token", tokens.cursor()))
        }
    })
}

pub(super) fn word() -> Rule<(Box<str>, Cursor)> {
    Rule::new(|tokens| {
        if !matches!(tokens.peek()?, Some(Token::Word { .. })) {
            return Err(ParseError::expected("a word", tokens.next_start()?));
        }
        let Token::Word { text, start } = tokens.commit() else {
            unreachable!("the peeked token was a word");
        };
        Ok((text.into(), start))
    })
}

pub(super) fn keyword(expected: &'static str) -> Rule<Cursor> {
    Rule::new(move |tokens| {
        if !matches!(tokens.peek()?, Some(Token::Word { text, .. }) if text == expected) {
            return Err(ParseError::expected(
                format_args!("{expected:?}"),
                tokens.next_start()?,
            ));
        }
        let token = tokens.commit();
        Ok(token.cursor())
    })
}

pub(super) fn symbol(expected: Symbol) -> Rule<Cursor> {
    Rule::new(move |tokens| {
        if !matches!(tokens.peek()?, Some(Token::Symbol { symbol, .. }) if *symbol == expected) {
            return Err(ParseError::expected(expected, tokens.next_start()?));
        }
        let token = tokens.commit();
        Ok(token.cursor())
    })
}

#[cfg(test)]
mod tests {
    use super::{Rule, adjacent, choice, eoi, keyword, recursive, sequence, symbol, word};
    use crate::input::StrInput;
    use crate::ito::Cursor;
    use crate::ito::lexer::{Lexer, Symbol};
    use std::rc::Rc;

    #[test]
    fn nested_parentheses_reuse_the_rule_built_once() {
        let mut builds = 0;
        let expr = recursive(|expr| {
            builds += 1;
            let group = sequence![symbol(Symbol::OpenParen), expr, symbol(Symbol::CloseParen)];
            choice([word(), group.map(|(_, inner, _)| inner)])
        });
        let file = sequence![expr, eoi()];

        let parsed = file
            .apply(&mut Lexer::new(&mut StrInput::new("((x))")))
            .unwrap();
        assert_eq!(
            parsed,
            (("x".into(), Cursor::default().advanced("((".chars())), ())
        );
        let error = file
            .apply(&mut Lexer::new(&mut StrInput::new("((x)")))
            .unwrap_err();
        assert_eq!(error.to_string(), "expected ) at line 1, column 5");
        assert_eq!(builds, 1);
    }

    #[test]
    fn dropping_a_recursive_rule_releases_its_body() {
        let leaf = Rc::new(word());
        let weak_leaf = Rc::downgrade(&leaf);
        let expr = recursive(|expr| {
            let prefixed = sequence![symbol(Symbol::Slash), expr].map(|(_, inner)| inner);
            choice([Rule::new(move |tokens| leaf.apply(tokens)), prefixed])
        });
        assert!(weak_leaf.upgrade().is_some());
        drop(expr);
        assert!(weak_leaf.upgrade().is_none());
    }

    #[test]
    fn a_mismatch_preserves_the_next_tokens_position_and_adjacency() {
        let mut input = StrInput::new("// 幅\r\nname");
        let mut tokens = Lexer::new(&mut input);

        let error = keyword("type").apply(&mut tokens).unwrap_err();
        let crate::ito::ParseError::Unexpected { cursor, .. } = error else {
            panic!("expected a keyword mismatch");
        };
        assert_eq!(tokens.cursor().offset, 0);
        assert_eq!(cursor.offset, "// 幅\r\n".len());
        assert!(adjacent().apply(&mut tokens).is_err());
        assert_eq!(
            word().apply(&mut tokens).unwrap(),
            (
                "name".into(),
                Cursor::default().advanced("// 幅\r\n".chars())
            )
        );
    }

    #[test]
    fn adjacency_checks_only_the_marked_boundary() {
        let rule = sequence![
            symbol(Symbol::OpenAngle),
            adjacent(),
            word(),
            symbol(Symbol::CloseAngle)
        ];
        let parsed = rule
            .apply(&mut Lexer::new(&mut StrInput::new("<Panel >")))
            .unwrap();
        assert_eq!(
            parsed,
            (
                Cursor::default(),
                (),
                ("Panel".into(), Cursor::default().advanced(['<'])),
                Cursor::default().advanced("<Panel ".chars())
            )
        );
        let error = rule
            .apply(&mut Lexer::new(&mut StrInput::new("< Panel>")))
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "expected an adjacent token at line 1, column 2"
        );
    }

    #[test]
    fn trailing_trivia_separates_the_last_word_from_eoi() {
        let mut input = StrInput::new("x// end");
        let mut tokens = Lexer::new(&mut input);

        assert_eq!(
            sequence![word(), eoi(), eoi()].apply(&mut tokens).unwrap(),
            (("x".into(), Cursor::default()), (), ())
        );
        assert_eq!(
            adjacent().apply(&mut tokens).unwrap_err().cursor().offset,
            1
        );
        assert_eq!(
            symbol(Symbol::CloseBrace)
                .apply(&mut tokens)
                .unwrap_err()
                .cursor()
                .offset,
            7
        );
    }

    #[test]
    fn word_keeps_the_spelling_and_start_regardless_of_identifier_rules() {
        let mut input = StrInput::new("9幅(");
        let mut tokens = Lexer::new(&mut input);

        assert_eq!(
            word().apply(&mut tokens).unwrap(),
            ("9幅".into(), Cursor::default())
        );

        let error = word().apply(&mut tokens).unwrap_err();
        assert_eq!(error.to_string(), "expected a word at line 1, column 3");
        symbol(Symbol::OpenParen).apply(&mut tokens).unwrap();
    }

    #[test]
    fn keyword_matches_a_whole_word() {
        let mut input = StrInput::new("typeName");
        let mut tokens = Lexer::new(&mut input);

        let error = keyword("type").apply(&mut tokens).unwrap_err();
        assert_eq!(error.to_string(), r#"expected "type" at line 1, column 1"#);

        let keyword = keyword("typeName").apply(&mut tokens).unwrap();
        assert_eq!(keyword, Cursor::default());
        eoi().apply(&mut tokens).unwrap();
    }

    #[test]
    fn symbol_reports_its_spelling_at_a_different_token_or_eoi() {
        let mut input = StrInput::new("(");
        let mut tokens = Lexer::new(&mut input);

        let close = symbol(Symbol::CloseParen);
        let error = close.apply(&mut tokens).unwrap_err();
        assert_eq!(error.to_string(), "expected ) at line 1, column 1");

        let open = symbol(Symbol::OpenParen).apply(&mut tokens).unwrap();
        assert_eq!(open, Cursor::default());
        let error = close.apply(&mut tokens).unwrap_err();
        assert_eq!(error.to_string(), "expected ) at line 1, column 2");
    }
}
