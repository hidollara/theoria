use super::super::lexer::TokenKind;
use super::{ParseError, Parser, Rule};

use crate::input::Cursor;

/// Read a word without interpreting its spelling.
pub fn word(description: &'static str) -> impl Rule<Output = (Box<str>, Cursor)> {
    WordRule(description)
}

struct WordRule(&'static str);

impl Rule for WordRule {
    type Output = (Box<str>, Cursor);

    fn description(&self) -> String {
        self.0.into()
    }

    fn read(&self, parser: &mut Parser<'_, '_>) -> Result<Option<Self::Output>, ParseError> {
        let token = parser.next_if(|kind| matches!(kind, TokenKind::Word(_)))?;
        let text = token.map(|token| {
            let TokenKind::Word(text) = token.kind else {
                unreachable!()
            };
            (text.into(), token.end)
        });
        Ok(text)
    }
}
