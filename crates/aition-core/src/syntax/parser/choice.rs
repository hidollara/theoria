use super::super::token::TokenStream;
use super::{ParseError, Parser};
use crate::lexer::LexemeIterator;

/// Try a nonempty list of candidates in order, stopping at a match or error.
/// All candidates must return the same output type.
macro_rules! choice {
    (@pairs $last_separator:literal; $first:expr, $second:expr, $($rest:expr),+) => {
        $crate::syntax::parser::choice!(@pairs ", or ";
            $crate::syntax::parser::choice_pair($first, $second, ", "), $($rest),+)
    };
    (@pairs $last_separator:literal; $first:expr, $second:expr) => {
        $crate::syntax::parser::choice_pair($first, $second, $last_separator)
    };
    (@pairs $last_separator:literal; $last:expr) => { $last };
    ($($parser:expr),+ $(,)?) => {
        $crate::syntax::parser::choice!(@pairs " or "; $($parser),+)
    };
}

pub(crate) use choice;

/// Binary choice used by the macro, preserving each parser's concrete type.
pub fn choice_pair<P: Parser, Q: Parser<Output = P::Output>>(
    first: P,
    second: Q,
    separator: &'static str,
) -> impl Parser<Output = P::Output> {
    Choice(first, second, separator)
}

struct Choice<P, Q>(P, Q, &'static str);

impl<P: Parser, Q: Parser<Output = P::Output>> Parser for Choice<P, Q> {
    type Output = P::Output;

    fn description(&self) -> String {
        format!("{}{}{}", self.0.description(), self.2, self.1.description())
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

#[cfg(test)]
mod tests;
