use super::super::token::TokenStream;
use super::{ParseError, Parser};
use crate::lexer::LexemeIterator;

/// Read zero or more matches, stopping at an unconsumed mismatch.
pub fn repeated<P: Parser>(parser: P) -> impl Parser<Output = Vec<P::Output>> {
    Repeated(parser)
}

struct Repeated<P>(P);

impl<P: Parser> Parser for Repeated<P> {
    type Output = Vec<P::Output>;

    fn description(&self) -> String {
        self.0.description()
    }

    fn read<'s, I: LexemeIterator<'s>>(
        &self,
        mut tokens: TokenStream<'s, I>,
    ) -> Result<(Option<Self::Output>, TokenStream<'s, I>), ParseError> {
        let mut values = Vec::new();
        loop {
            let consumed = tokens.consumed();
            let (value, rest) = self.0.read(tokens)?;
            tokens = rest;
            let Some(value) = value else {
                return Ok((Some(values), tokens));
            };
            assert!(
                tokens.consumed() > consumed,
                "a repeated parser must consume a token"
            );
            values.push(value);
        }
    }
}

#[cfg(test)]
mod tests;
