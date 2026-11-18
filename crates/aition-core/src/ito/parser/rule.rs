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
