use super::super::lexer::{Symbol, TokenKind};
use super::{ParseError, Parser, Rule};

use crate::input::Cursor;

/// Recognize and consume one specified symbol, returning its end cursor on a match.
/// A different token or end of input is an unconsumed mismatch.
pub fn symbol(symbol: Symbol) -> impl Rule<Output = Cursor> {
    SymbolRule(symbol)
}

struct SymbolRule(Symbol);

impl Rule for SymbolRule {
    type Output = Cursor;

    fn description(&self) -> String {
        format!("'{}'", self.0)
    }

    fn read(&self, parser: &mut Parser<'_, '_>) -> Result<Option<Self::Output>, ParseError> {
        let token =
            parser.next_if(|kind| matches!(kind, TokenKind::Symbol(found) if *found == self.0))?;
        Ok(token.map(|token| token.end))
    }
}

#[cfg(test)]
mod tests;
