use super::{ParseError, Parser, Rule};

/// The empty production (ε). Succeeds without reading the next token.
pub(super) fn empty() -> impl Rule<Output = ()> {
    Empty
}

struct Empty;

impl Rule for Empty {
    type Output = ();

    fn description(&self) -> String {
        "empty".into()
    }

    fn read(&self, _: &mut Parser<'_, '_>) -> Result<Option<()>, ParseError> {
        Ok(Some(()))
    }
}

/// Ordered choice. Only an unconsumed mismatch reaches the second alternative.
pub(super) fn or<R: Rule, Q: Rule<Output = R::Output>>(first: R, second: Q) -> Or<R, Q> {
    Or {
        first,
        second,
        separator: " or ",
    }
}

pub(super) struct Or<R, Q> {
    first: R,
    second: Q,
    separator: &'static str,
}

impl<R, Q> Or<R, Q> {
    pub(super) fn separator(mut self, separator: &'static str) -> Self {
        self.separator = separator;
        self
    }
}

impl<R: Rule, Q: Rule<Output = R::Output>> Rule for Or<R, Q> {
    type Output = R::Output;

    fn description(&self) -> String {
        format!(
            "{}{}{}",
            self.first.description(),
            self.separator,
            self.second.description()
        )
    }

    fn read(&self, parser: &mut Parser<'_, '_>) -> Result<Option<Self::Output>, ParseError> {
        match self.first.read(parser)? {
            Some(value) => Ok(Some(value)),
            None => self.second.read(parser),
        }
    }
}

/// Defer constructing a rule until it is read or described.
pub(super) fn defer<F>(factory: F) -> Deferred<F> {
    Deferred(factory)
}

pub(super) struct Deferred<F>(F);

impl<F, R> Rule for Deferred<F>
where
    F: Fn() -> R,
    R: Rule,
{
    type Output = R::Output;

    fn description(&self) -> String {
        (self.0)().description()
    }

    fn read(&self, parser: &mut Parser<'_, '_>) -> Result<Option<Self::Output>, ParseError> {
        (self.0)().read(parser)
    }
}

#[cfg(test)]
mod tests {
    use super::{defer, empty, or};
    use crate::ito::lexer::{self, Symbol};
    use crate::ito::parser::{Parser, Rule, repeated, rule, symbol};

    #[test]
    fn empty_succeeds_without_reading_and_preempts_the_next_alternative() {
        let mut stream = || panic!("unexpected lexer read");
        let mut parser = Parser::new(&mut stream);
        let result = or(empty(), symbol(Symbol::Dot).map(|_| ()))
            .read(&mut parser)
            .unwrap();
        assert_eq!(result, Some(()));
        assert_eq!(parser.consumed(), 0);
    }

    #[test]
    fn repetition_agrees_with_its_recursive_grammar() {
        fn dots() -> impl Rule<Output = usize> {
            or(
                rule("'.'", |parser| {
                    parser.read(symbol(Symbol::Dot))?;
                    Ok(1 + parser.read(defer(dots))?)
                }),
                empty().map(|()| 0),
            )
        }

        for source in ["", ";", ".", "...;"] {
            let mut stream = lexer::from_str(source);
            let mut recursive = Parser::new(&mut stream);
            let mut stream = lexer::from_str(source);
            let mut iterative = Parser::new(&mut stream);
            let count = recursive.read(defer(dots)).unwrap();
            let items = iterative.read(repeated(symbol(Symbol::Dot))).unwrap();
            assert_eq!(count, items.len());
            assert_eq!(recursive.cursor().unwrap(), iterative.cursor().unwrap());
        }
    }
}
