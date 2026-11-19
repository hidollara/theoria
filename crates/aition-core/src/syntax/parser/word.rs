use super::super::token::{TokenKind, TokenStream};
use super::{ParseError, Parser};
use crate::lexer::LexemeIterator;

/// Read a word without interpreting its spelling.
pub fn word(description: &'static str) -> impl Parser<Output = Box<str>> {
    WordParser(description)
}

struct WordParser(&'static str);

impl Parser for WordParser {
    type Output = Box<str>;

    fn description(&self) -> String {
        self.0.into()
    }

    fn read<'s, I: LexemeIterator<'s>>(
        &self,
        mut tokens: TokenStream<'s, I>,
    ) -> Result<(Option<Self::Output>, TokenStream<'s, I>), ParseError> {
        let token = tokens.next_if(|kind| matches!(kind, TokenKind::Word(_)))?;
        let text = token.map(|token| {
            let TokenKind::Word(text) = token.kind else {
                unreachable!()
            };
            text.into()
        });
        Ok((text, tokens))
    }
}
