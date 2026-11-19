use super::{ParseError, Parser, Rule, empty, or};

/// Read `R = (item R) or ε` iteratively, with one consuming item per step.
pub fn repeated<R: Rule>(rule: R) -> impl Rule<Output = Vec<R::Output>> {
    Repeated(rule)
}

struct Repeated<R>(R);

impl<R: Rule> Rule for Repeated<R> {
    type Output = Vec<R::Output>;

    fn description(&self) -> String {
        self.0.description()
    }

    fn read(&self, parser: &mut Parser<'_, '_>) -> Result<Option<Self::Output>, ParseError> {
        let mut values = Vec::new();
        let step = or((&self.0).map(Some), empty().map(|()| None));
        loop {
            let consumed = parser.consumed();
            let value = parser.read(&step)?;
            let Some(value) = value else {
                return Ok(Some(values));
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
mod tests;
