use super::label;
use super::map;
use super::{ParseError, Parser};

/// A grammar rule evaluated against a parser at its current input position.
/// `Ok(None)` leaves input untouched. `Ok(Some(value))` can succeed even at the
/// same position (ε). `Err` terminates the parse.
pub trait Rule: Sized {
    type Output;

    /// Describe the construct this rule recognizes.
    /// Used to report an unconsumed mismatch; errors after consumption describe
    /// the syntax needed at the failure position in `read` itself.
    fn description(&self) -> String;
    fn read(&self, parser: &mut Parser<'_, '_>) -> Result<Option<Self::Output>, ParseError>;

    /// Name an unconsumed mismatch without replacing errors within the construct.
    fn label(self, description: &'static str) -> impl Rule<Output = Self::Output> {
        label::label(self, description)
    }

    /// Transform a matched result without changing the consumed input.
    fn map<O>(self, f: impl Fn(Self::Output) -> O) -> impl Rule<Output = O> {
        map::map(self, f)
    }

    /// Validate a matched result, reporting rejection at its starting position.
    fn try_map<O>(
        self,
        description: &'static str,
        f: impl Fn(Self::Output) -> Option<O>,
    ) -> impl Rule<Output = O> {
        map::try_map(self, description, f)
    }
}

/// Borrow a rule when a required read reuses an existing grammar component.
impl<R: Rule> Rule for &R {
    type Output = R::Output;

    fn description(&self) -> String {
        R::description(self)
    }

    fn read(&self, parser: &mut Parser<'_, '_>) -> Result<Option<Self::Output>, ParseError> {
        R::read(self, parser)
    }
}
