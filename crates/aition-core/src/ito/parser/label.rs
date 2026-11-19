use super::{ParseError, Parser, Rule};

pub(super) fn label<R: Rule>(rule: R, description: &'static str) -> impl Rule<Output = R::Output> {
    Label(rule, description)
}

struct Label<R>(R, &'static str);

impl<R: Rule> Rule for Label<R> {
    type Output = R::Output;

    fn description(&self) -> String {
        self.1.into()
    }

    fn read(&self, parser: &mut Parser<'_, '_>) -> Result<Option<Self::Output>, ParseError> {
        self.0.read(parser)
    }
}
