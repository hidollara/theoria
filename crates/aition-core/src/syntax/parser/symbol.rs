use super::super::token::{TokenKind, TokenStream};
use super::{ParseError, Parser};
use crate::lexer::{LexemeIterator, Symbol};

/// Recognize and consume one specified symbol, returning `()` on a match.
/// A different token or EOF is an unconsumed mismatch.
pub fn symbol(symbol: Symbol) -> impl Parser<Output = ()> {
    SymbolParser(symbol)
}

struct SymbolParser(Symbol);

impl Parser for SymbolParser {
    type Output = ();

    fn description(&self) -> String {
        format!("'{}'", self.0)
    }

    fn read<'s, I: LexemeIterator<'s>>(
        &self,
        mut tokens: TokenStream<'s, I>,
    ) -> Result<(Option<Self::Output>, TokenStream<'s, I>), ParseError> {
        let token =
            tokens.next_if(|kind| matches!(kind, TokenKind::Symbol(found) if *found == self.0))?;
        Ok((token.map(|_| ()), tokens))
    }
}

#[cfg(test)]
mod tests;
