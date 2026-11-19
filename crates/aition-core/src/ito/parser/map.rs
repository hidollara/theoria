use super::{ParseError, Parser, Rule};

pub(super) fn map<R: Rule, O>(rule: R, f: impl Fn(R::Output) -> O) -> impl Rule<Output = O> {
    Map(rule, f)
}

struct Map<R, F>(R, F);

impl<R: Rule, O, F: Fn(R::Output) -> O> Rule for Map<R, F> {
    type Output = O;

    fn description(&self) -> String {
        self.0.description()
    }

    fn read(&self, parser: &mut Parser<'_, '_>) -> Result<Option<Self::Output>, ParseError> {
        let value = self.0.read(parser)?;
        Ok(value.map(&self.1))
    }
}

pub(super) fn try_map<R: Rule, O>(
    rule: R,
    description: &'static str,
    f: impl Fn(R::Output) -> Option<O>,
) -> impl Rule<Output = O> {
    TryMap(rule, description, f)
}

struct TryMap<R, F>(R, &'static str, F);

impl<R: Rule, O, F: Fn(R::Output) -> Option<O>> Rule for TryMap<R, F> {
    type Output = O;

    fn description(&self) -> String {
        self.1.into()
    }

    fn read(&self, parser: &mut Parser<'_, '_>) -> Result<Option<Self::Output>, ParseError> {
        let start = parser.cursor()?;
        let value = self.0.read(parser)?;
        let Some(value) = value else {
            return Ok(None);
        };
        let Some(value) = (self.2)(value) else {
            return Err(ParseError::expected(self.description(), start));
        };
        Ok(Some(value))
    }
}
