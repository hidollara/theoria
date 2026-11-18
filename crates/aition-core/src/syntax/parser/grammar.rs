//! Typed grammar descriptions. A miss consumes no tokens; an error is committed.

use std::marker::PhantomData;

use crate::lexer::{LexemeIterator, Symbol};

use super::{
    Expected, Parsable, ParseError,
    node::AstNode,
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
    fn or<Q: Parser<Output = Self::Output>>(self, other: Q) -> Or<Self, Q> {
        Or(self, other)
    }
    fn optional(self) -> Optional<Self> {
        Optional(self)
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

    /// Each matching suffix constructs a parent around the accumulated node.
    fn fold<T, Q: Parser, F: Fn(AstNode<T>, Q::Output) -> T>(
        self,
        suffix: Q,
        f: F,
    ) -> Fold<Self, Q, F>
    where
        Self: Parser<Output = AstNode<T>>,
    {
        Fold(self, suffix, f)
    }

    fn labelled(self, expected: Expected) -> Labelled<Self> {
        Labelled(self, expected)
    }
}

pub struct Then<P, Q>(P, Q);
pub struct Or<P, Q>(P, Q);
pub struct Optional<P>(P);
pub struct Repeated<P>(P);
pub struct Map<P, F>(P, F);
pub struct TryMap<P, F>(P, Expected, F);
pub struct Fold<P, Q, F>(P, Q, F);
pub struct Labelled<P>(P, Expected);
pub struct Rule<T>(PhantomData<fn() -> T>);
pub struct SymbolParser(Symbol);
pub struct Word(Expected);
pub struct Separated<P> {
    item: P,
    separator: Symbol,
    end: Symbol,
    allow_empty: bool,
}

pub fn rule<T: Parsable>() -> Rule<T> {
    Rule(PhantomData)
}
pub fn symbol(symbol: Symbol) -> SymbolParser {
    SymbolParser(symbol)
}
pub fn word(expected: Expected) -> Word {
    Word(expected)
}

pub fn surrounded_by<P: Parser>(
    parser: P,
    open: Symbol,
    close: Symbol,
) -> impl Parser<Output = P::Output> {
    symbol(open)
        .then(parser)
        .then(symbol(close))
        .map(|((_, value), _)| value)
}

/// A comma-like list ending before `end`, allowing a trailing separator.
pub fn separated_until<P: Parser>(
    item: P,
    separator: Symbol,
    end: Symbol,
    allow_empty: bool,
) -> Separated<P> {
    Separated {
        item,
        separator,
        end,
        allow_empty,
    }
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

impl<P: Parser, Q: Parser<Output = P::Output>> Parser for Or<P, Q> {
    type Output = P::Output;
    fn expected(&self) -> Expected {
        self.0.expected()
    }
    fn read<'s, I: LexemeIterator<'s>>(
        &self,
        tokens: TokenStream<'s, I>,
    ) -> Result<(Option<Self::Output>, TokenStream<'s, I>), ParseError> {
        let (value, tokens) = self.0.read(tokens)?;
        if value.is_some() {
            Ok((value, tokens))
        } else {
            self.1.read(tokens)
        }
    }
}

impl<P: Parser> Parser for Optional<P> {
    type Output = Option<P::Output>;
    fn expected(&self) -> Expected {
        self.0.expected()
    }
    fn read<'s, I: LexemeIterator<'s>>(
        &self,
        tokens: TokenStream<'s, I>,
    ) -> Result<(Option<Self::Output>, TokenStream<'s, I>), ParseError> {
        let (value, tokens) = self.0.read(tokens)?;
        Ok((Some(value), tokens))
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

impl<T, P: Parser<Output = AstNode<T>>, Q: Parser, F: Fn(AstNode<T>, Q::Output) -> T> Parser
    for Fold<P, Q, F>
{
    type Output = AstNode<T>;
    fn expected(&self) -> Expected {
        self.0.expected()
    }
    fn read<'s, I: LexemeIterator<'s>>(
        &self,
        mut tokens: TokenStream<'s, I>,
    ) -> Result<(Option<Self::Output>, TokenStream<'s, I>), ParseError> {
        let mark = tokens.mark()?;
        let (value, mut tokens) = self.0.read(tokens)?;
        let Some(mut value) = value else {
            return Ok((None, tokens));
        };
        loop {
            let consumed = tokens.consumed();
            let (suffix, rest) = self.1.read(tokens)?;
            tokens = rest;
            let Some(suffix) = suffix else {
                return Ok((Some(value), tokens));
            };
            assert!(
                tokens.consumed() > consumed,
                "a folded grammar must consume a token"
            );
            value = AstNode {
                value: (self.2)(value, suffix),
                span: Some(tokens.span(&mark)),
            };
        }
    }
}

impl<P: Parser> Parser for Labelled<P> {
    type Output = P::Output;
    fn expected(&self) -> Expected {
        self.1.clone()
    }
    fn read<'s, I: LexemeIterator<'s>>(
        &self,
        tokens: TokenStream<'s, I>,
    ) -> Result<(Option<Self::Output>, TokenStream<'s, I>), ParseError> {
        self.0.read(tokens)
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

impl<P: Parser> Parser for Separated<P> {
    type Output = Vec<P::Output>;
    fn expected(&self) -> Expected {
        self.item.expected()
    }
    fn read<'s, I: LexemeIterator<'s>>(
        &self,
        mut tokens: TokenStream<'s, I>,
    ) -> Result<(Option<Self::Output>, TokenStream<'s, I>), ParseError> {
        let at_end = |tokens: &mut TokenStream<'s, I>| -> Result<bool, ParseError> {
            Ok(tokens
                .peek()?
                .is_some_and(|token| token.kind == TokenKind::Symbol(self.end)))
        };
        if self.allow_empty && at_end(&mut tokens)? {
            return Ok((Some(Vec::new()), tokens));
        }
        let (first, rest) = self.item.read(tokens)?;
        tokens = rest;
        let Some(first) = first else {
            return Ok((None, tokens));
        };
        let mut values = vec![first];
        while tokens.eat(self.separator)? {
            if at_end(&mut tokens)? {
                break;
            }
            let (value, rest) = self.item.read(tokens)?;
            tokens = rest;
            let Some(value) = value else {
                return Err(tokens.error(self.item.expected()));
            };
            values.push(value);
        }
        Ok((Some(values), tokens))
    }
}

#[cfg(test)]
mod tests;
