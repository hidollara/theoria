use super::super::token::TokenStream;
use super::{ParseError, Parser};
use crate::lexer::LexemeIterator;

/// Accept absence, but propagate errors from a partially read construct.
pub fn optional<P: Parser>(parser: P) -> impl Parser<Output = Option<P::Output>> {
    Optional(parser)
}

struct Optional<P>(P);

impl<P: Parser> Parser for Optional<P> {
    type Output = Option<P::Output>;

    fn description(&self) -> String {
        self.0.description()
    }

    fn read<'s, I: LexemeIterator<'s>>(
        &self,
        tokens: TokenStream<'s, I>,
    ) -> Result<(Option<Self::Output>, TokenStream<'s, I>), ParseError> {
        let (value, tokens) = self.0.read(tokens)?;
        Ok((Some(value), tokens))
    }
}
