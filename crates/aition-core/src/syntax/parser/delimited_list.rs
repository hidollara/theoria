use super::super::token::{TokenKind, TokenStream};
use super::{ParseError, Parser};
use crate::lexer::{LexemeIterator, Symbol};

/// Read a separated list including its opening and closing delimiters.
/// A trailing separator is allowed; an empty list depends on `allow_empty`.
pub fn delimited_list<P: Parser>(
    item: P,
    open: Symbol,
    close: Symbol,
    separator: Symbol,
    allow_empty: bool,
) -> impl Parser<Output = Vec<P::Output>> {
    DelimitedList {
        item,
        open,
        close,
        separator,
        allow_empty,
    }
}

struct DelimitedList<P> {
    item: P,
    open: Symbol,
    close: Symbol,
    separator: Symbol,
    allow_empty: bool,
}

impl<P: Parser> Parser for DelimitedList<P> {
    type Output = Vec<P::Output>;

    fn description(&self) -> String {
        format!("'{}'", self.open)
    }

    fn read<'s, I: LexemeIterator<'s>>(
        &self,
        mut tokens: TokenStream<'s, I>,
    ) -> Result<(Option<Self::Output>, TokenStream<'s, I>), ParseError> {
        if tokens
            .next_if(|kind| matches!(kind, TokenKind::Symbol(found) if *found == self.open))?
            .is_none()
        {
            return Ok((None, tokens));
        }
        let at_close =
            |kind: &TokenKind<'s>| matches!(kind, TokenKind::Symbol(found) if *found == self.close);
        if self.allow_empty && tokens.next_if(at_close)?.is_some() {
            return Ok((Some(Vec::new()), tokens));
        }
        let mut values = Vec::new();
        loop {
            let (value, rest) = self.item.read(tokens)?;
            tokens = rest;
            let Some(value) = value else {
                return Err(ParseError::expected(
                    self.item.description(),
                    tokens.peek()?.cursor(),
                ));
            };
            values.push(value);
            if tokens.next_if(at_close)?.is_some() {
                return Ok((Some(values), tokens));
            }
            if tokens
                .next_if(
                    |kind| matches!(kind, TokenKind::Symbol(found) if *found == self.separator),
                )?
                .is_none()
            {
                return Err(ParseError::expected(
                    format!("'{}'", self.close),
                    tokens.peek()?.cursor(),
                ));
            }
            if tokens.next_if(at_close)?.is_some() {
                return Ok((Some(values), tokens));
            }
        }
    }
}
