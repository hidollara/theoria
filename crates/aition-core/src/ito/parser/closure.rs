use super::{ParseError, Parser, Rule};

/// Build a rule from sequential reads. A syntax error before consuming any
/// token is a mismatch; lexical and input errors always propagate.
pub fn rule<T>(
    description: &'static str,
    read: impl Fn(&mut Parser<'_, '_>) -> Result<T, ParseError>,
) -> impl Rule<Output = T> {
    ClosureRule { description, read }
}

struct ClosureRule<F> {
    description: &'static str,
    read: F,
}

impl<T, F: Fn(&mut Parser<'_, '_>) -> Result<T, ParseError>> Rule for ClosureRule<F> {
    type Output = T;

    fn description(&self) -> String {
        self.description.into()
    }

    fn read(&self, parser: &mut Parser<'_, '_>) -> Result<Option<T>, ParseError> {
        parser.evaluate(&self.read)
    }
}

#[cfg(test)]
mod tests;
