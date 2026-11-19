use super::{ParseError, Parser, Rule};
use crate::input::Cursor;
use crate::syntax::ast::{NodeSource, Trivia};

/// Start the span at rule entry and end it at the cursor returned by the rule.
pub(super) fn located<T>(
    rule: impl Rule<Output = (T, Vec<Trivia>, Cursor)>,
) -> impl Rule<Output = (T, NodeSource)> {
    Located(rule)
}

struct Located<R>(R);

impl<T, R: Rule<Output = (T, Vec<Trivia>, Cursor)>> Rule for Located<R> {
    type Output = (T, NodeSource);

    fn description(&self) -> String {
        self.0.description()
    }

    fn read(&self, parser: &mut Parser<'_, '_>) -> Result<Option<Self::Output>, ParseError> {
        let start = parser.cursor()?;
        Ok(self
            .0
            .read(parser)?
            .map(|(value, trivia, end)| (value, NodeSource { start, end, trivia })))
    }
}

#[cfg(test)]
mod tests;
