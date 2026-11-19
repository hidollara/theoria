use super::super::token::TokenStream;
use super::{ParseError, Parser};
use crate::lexer::LexemeIterator;

pub(super) fn map<P: Parser, O>(parser: P, f: impl Fn(P::Output) -> O) -> impl Parser<Output = O> {
    Map(parser, f)
}

struct Map<P, F>(P, F);

impl<P: Parser, O, F: Fn(P::Output) -> O> Parser for Map<P, F> {
    type Output = O;

    fn description(&self) -> String {
        self.0.description()
    }

    fn read<'s, I: LexemeIterator<'s>>(
        &self,
        tokens: TokenStream<'s, I>,
    ) -> Result<(Option<Self::Output>, TokenStream<'s, I>), ParseError> {
        let (value, tokens) = self.0.read(tokens)?;
        Ok((value.map(&self.1), tokens))
    }
}

pub(super) fn try_map<P: Parser, O>(
    parser: P,
    description: &'static str,
    f: impl Fn(P::Output) -> Option<O>,
) -> impl Parser<Output = O> {
    TryMap(parser, description, f)
}

struct TryMap<P, F>(P, &'static str, F);

impl<P: Parser, O, F: Fn(P::Output) -> Option<O>> Parser for TryMap<P, F> {
    type Output = O;

    fn description(&self) -> String {
        self.1.into()
    }

    fn read<'s, I: LexemeIterator<'s>>(
        &self,
        mut tokens: TokenStream<'s, I>,
    ) -> Result<(Option<Self::Output>, TokenStream<'s, I>), ParseError> {
        let start = tokens.peek()?.cursor();
        let (value, tokens) = self.0.read(tokens)?;
        let Some(value) = value else {
            return Ok((None, tokens));
        };
        let Some(value) = (self.2)(value) else {
            return Err(ParseError::expected(self.description(), start));
        };
        Ok((Some(value), tokens))
    }
}
