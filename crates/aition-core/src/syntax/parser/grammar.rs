//! Typed grammar descriptions. A miss consumes no tokens; an error is committed.

use std::marker::PhantomData;

use crate::lexer::{LexemeIterator, Symbol};

use super::{
    Expected, Parsable, ParseError,
    token::{TokenKind, TokenStream},
};

/// A grammar description and its construction operations.
///
/// `read` returns the stream on a match or an unconsumed miss, but never on error.
pub trait Parser: Sized {
    type Output;

    fn expected(&self) -> Expected;

    fn read<'s, I: LexemeIterator<'s>>(
        &self,
        tokens: TokenStream<'s, I>,
    ) -> Result<(Option<Self::Output>, TokenStream<'s, I>), ParseError>;

    fn then<Q: Parser>(self, next: Q) -> Then<Self, Q> {
        Then(self, next)
    }
    fn repeated(self) -> Repeated<Self> {
        Repeated(self)
    }

    /// Transform the result without adding a syntax node.
    fn map<O, F: Fn(Self::Output) -> O>(self, f: F) -> Map<Self, F> {
        Map(self, f)
    }

    fn try_map<O, F: Fn(Self::Output) -> Option<O>>(
        self,
        expected: Expected,
        f: F,
    ) -> TryMap<Self, F> {
        TryMap(self, expected, f)
    }
}

pub struct Then<P, Q>(P, Q);
pub struct Repeated<P>(P);
pub struct Map<P, F>(P, F);
pub struct TryMap<P, F>(P, Expected, F);
pub struct Rule<T>(PhantomData<fn() -> T>);
pub struct SymbolParser(Symbol);
pub struct Word(Expected);

pub fn rule<T: Parsable>() -> Rule<T> {
    Rule(PhantomData)
}
pub fn symbol(symbol: Symbol) -> SymbolParser {
    SymbolParser(symbol)
}
pub fn word(expected: Expected) -> Word {
    Word(expected)
}

impl<P: Parser, Q: Parser> Parser for Then<P, Q> {
    type Output = (P::Output, Q::Output);
    fn expected(&self) -> Expected {
        self.0.expected()
    }
    fn read<'s, I: LexemeIterator<'s>>(
        &self,
        tokens: TokenStream<'s, I>,
    ) -> Result<(Option<Self::Output>, TokenStream<'s, I>), ParseError> {
        let consumed = tokens.consumed();
        let (first, tokens) = self.0.read(tokens)?;
        let Some(first) = first else {
            return Ok((None, tokens));
        };
        let (second, mut tokens) = self.1.read(tokens)?;
        match second {
            Some(second) => Ok((Some((first, second)), tokens)),
            None if tokens.consumed() == consumed => Ok((None, tokens)),
            None => Err(tokens.error(self.1.expected())),
        }
    }
}

impl<P: Parser> Parser for Repeated<P> {
    type Output = Vec<P::Output>;
    fn expected(&self) -> Expected {
        self.0.expected()
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
                "a repeated grammar must consume a token"
            );
            values.push(value);
        }
    }
}

impl<P: Parser, O, F: Fn(P::Output) -> O> Parser for Map<P, F> {
    type Output = O;
    fn expected(&self) -> Expected {
        self.0.expected()
    }
    fn read<'s, I: LexemeIterator<'s>>(
        &self,
        tokens: TokenStream<'s, I>,
    ) -> Result<(Option<O>, TokenStream<'s, I>), ParseError> {
        let (value, tokens) = self.0.read(tokens)?;
        Ok((value.map(&self.1), tokens))
    }
}

impl<P: Parser, O, F: Fn(P::Output) -> Option<O>> Parser for TryMap<P, F> {
    type Output = O;
    fn expected(&self) -> Expected {
        self.1.clone()
    }
    fn read<'s, I: LexemeIterator<'s>>(
        &self,
        mut tokens: TokenStream<'s, I>,
    ) -> Result<(Option<Self::Output>, TokenStream<'s, I>), ParseError> {
        let mark = tokens.mark()?;
        let (value, tokens) = self.0.read(tokens)?;
        let Some(value) = value else {
            return Ok((None, tokens));
        };
        let span = tokens.span(&mark);
        let Some(value) = (self.2)(value) else {
            return Err(ParseError::Unexpected {
                span: span.range,
                cursor: span.start,
                expected: self.1.clone(),
            });
        };
        Ok((Some(value), tokens))
    }
}

impl<T: Parsable> Parser for Rule<T> {
    type Output = T;
    fn expected(&self) -> Expected {
        T::parser().expected()
    }
    fn read<'s, I: LexemeIterator<'s>>(
        &self,
        tokens: TokenStream<'s, I>,
    ) -> Result<(Option<T>, TokenStream<'s, I>), ParseError> {
        T::parser().read(tokens)
    }
}

impl Parser for SymbolParser {
    type Output = ();
    fn expected(&self) -> Expected {
        Expected::Symbol(self.0)
    }
    fn read<'s, I: LexemeIterator<'s>>(
        &self,
        mut tokens: TokenStream<'s, I>,
    ) -> Result<(Option<()>, TokenStream<'s, I>), ParseError> {
        let value = tokens.eat(self.0)?.then_some(());
        Ok((value, tokens))
    }
}

impl Parser for Word {
    type Output = Box<str>;
    fn expected(&self) -> Expected {
        self.0.clone()
    }
    fn read<'s, I: LexemeIterator<'s>>(
        &self,
        mut tokens: TokenStream<'s, I>,
    ) -> Result<(Option<Self::Output>, TokenStream<'s, I>), ParseError> {
        if !tokens
            .peek()?
            .is_some_and(|token| matches!(token.kind, TokenKind::Word(_)))
        {
            return Ok((None, tokens));
        }
        let TokenKind::Word(text) = tokens.next()?.unwrap().kind else {
            unreachable!()
        };
        Ok((Some(text.into()), tokens))
    }
}

#[cfg(test)]
mod tests;
