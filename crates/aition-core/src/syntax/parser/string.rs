use super::super::token::{TokenKind, TokenStream};
use super::{ParseError, Parser};
use crate::lexer::LexemeIterator;

/// Read quoted text without interpreting escapes or the literal's target type.
pub fn string() -> impl Parser<Output = Box<str>> {
    StringParser
}

struct StringParser;

impl Parser for StringParser {
    type Output = Box<str>;

    fn description(&self) -> String {
        "a string literal".into()
    }

    fn read<'s, I: LexemeIterator<'s>>(
        &self,
        mut tokens: TokenStream<'s, I>,
    ) -> Result<(Option<Self::Output>, TokenStream<'s, I>), ParseError> {
        let token = tokens.next_if(|kind| matches!(kind, TokenKind::String(_)))?;
        let text = token.map(|token| {
            let TokenKind::String(text) = token.kind else {
                unreachable!()
            };
            text.into()
        });
        Ok((text, tokens))
    }
}
