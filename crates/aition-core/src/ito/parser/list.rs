use super::super::lexer::Symbol;
use super::{ParseError, Parser, Rule, repeated, symbol, trivia};

use crate::{input::Cursor, syntax::ast::Trivia};

pub(super) struct ParsedList<T> {
    pub items: Vec<T>,
    pub trivia: Vec<Trivia>,
    pub end: Cursor,
}

/// Read a separated list including its opening and closing delimiters.
/// A trailing separator is allowed; an empty list depends on `allow_empty`.
pub(super) fn list<R: Rule>(
    item: R,
    open: Symbol,
    close: Symbol,
    separator: Symbol,
    allow_empty: bool,
) -> impl Rule<Output = ParsedList<R::Output>> {
    List {
        item,
        open,
        close,
        separator,
        allow_empty,
    }
}

struct List<R> {
    item: R,
    open: Symbol,
    close: Symbol,
    separator: Symbol,
    allow_empty: bool,
}

impl<R: Rule> Rule for List<R> {
    type Output = ParsedList<R::Output>;

    fn description(&self) -> String {
        format!("'{}'", self.open)
    }

    fn read(&self, parser: &mut Parser<'_, '_>) -> Result<Option<Self::Output>, ParseError> {
        if symbol(self.open).read(parser)?.is_none() {
            return Ok(None);
        }
        let mut gaps = parser.read(repeated(trivia()))?;
        let close = symbol(self.close);
        if self.allow_empty
            && let Some(end) = close.read(parser)?
        {
            return Ok(Some(ParsedList {
                items: Vec::new(),
                trivia: gaps,
                end,
            }));
        }
        let mut values = Vec::new();
        loop {
            values.push(parser.read(&self.item)?);
            gaps.extend(parser.read(repeated(trivia()))?);
            if let Some(end) = close.read(parser)? {
                return Ok(Some(ParsedList {
                    items: values,
                    trivia: gaps,
                    end,
                }));
            }
            if symbol(self.separator).read(parser)?.is_none() {
                return Err(ParseError::expected(
                    format!("'{}'", self.close),
                    parser.cursor()?,
                ));
            }
            gaps.extend(parser.read(repeated(trivia()))?);
            if let Some(end) = close.read(parser)? {
                return Ok(Some(ParsedList {
                    items: values,
                    trivia: gaps,
                    end,
                }));
            }
        }
    }
}
