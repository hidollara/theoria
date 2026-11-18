use super::super::lexer::TokenKind;
use super::{ParseError, Parser, Rule};
use crate::syntax::ast::{Trivia, TriviaKind};

/// Read one whitespace or line comment token.
pub fn trivia() -> impl Rule<Output = Trivia> {
    TriviaRule
}

struct TriviaRule;

impl Rule for TriviaRule {
    type Output = Trivia;

    fn description(&self) -> String {
        "whitespace or a comment".into()
    }

    fn read(&self, parser: &mut Parser<'_, '_>) -> Result<Option<Trivia>, ParseError> {
        let token = parser
            .next_if(|kind| matches!(kind, TokenKind::Whitespace(_) | TokenKind::LineComment(_)))?;
        Ok(token.map(|token| Trivia {
            kind: match token.kind {
                TokenKind::Whitespace(text) => TriviaKind::Whitespace(text.into()),
                TokenKind::LineComment(text) => TriviaKind::LineComment(text.into()),
                _ => unreachable!(),
            },
            start: token.start,
            end: token.end,
        }))
    }
}
