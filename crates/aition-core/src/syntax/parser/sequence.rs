use super::super::token::TokenStream;
use super::{ParseError, Parser};
use crate::lexer::LexemeIterator;

/// Read a nonempty sequence and return its results as a flat tuple.
macro_rules! sequence {
    // Each expansion introduces a fresh binding, even though all are spelled `value`.
    (@collect [$($parsers:expr,)*] [$($values:ident,)*]; $next:expr $(, $rest:expr)*) => {
        $crate::syntax::parser::sequence!(@collect
            [$($parsers,)* $next,] [$($values,)* value,]; $($rest),*)
    };
    (@collect [$($parsers:expr,)+] [$($values:ident,)+];) => {
        $crate::syntax::parser::Parser::map(
            $crate::syntax::parser::sequence!(@pairs $($parsers),+),
            |$crate::syntax::parser::sequence!(@pattern $($values),+)| ($($values,)+),
        )
    };
    (@pairs $first:expr, $second:expr $(, $rest:expr)*) => {
        $crate::syntax::parser::sequence!(@pairs
            $crate::syntax::parser::sequence_pair($first, $second) $(, $rest)*)
    };
    (@pairs $last:expr) => { $last };
    (@pattern $first:pat, $second:pat $(, $rest:pat)*) => {
        $crate::syntax::parser::sequence!(@pattern ($first, $second) $(, $rest)*)
    };
    (@pattern $last:pat) => { $last };
    ($($parser:expr),+ $(,)?) => {
        $crate::syntax::parser::sequence!(@collect [] []; $($parser),+)
    };
}

pub(crate) use sequence;

/// Binary sequencing used by the macro; no tuple-length-specific impls are needed.
pub fn sequence_pair<P: Parser, Q: Parser>(
    first: P,
    second: Q,
) -> impl Parser<Output = (P::Output, Q::Output)> {
    Pair(first, second)
}

struct Pair<P, Q>(P, Q);

impl<P: Parser, Q: Parser> Parser for Pair<P, Q> {
    type Output = (P::Output, Q::Output);

    fn description(&self) -> String {
        self.0.description()
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
            None => Err(ParseError::expected(
                self.1.description(),
                tokens.peek()?.cursor(),
            )),
        }
    }
}

#[cfg(test)]
mod tests;
