use super::{Cursor, Symbol};
use std::borrow::Cow;

#[derive(Debug, PartialEq, Eq)]
pub enum Token<'s> {
    Word { text: Cow<'s, str>, start: Cursor },
    String(Quoted<'s>),
    Symbol { symbol: Symbol, start: Cursor },
}

#[derive(Debug, PartialEq, Eq)]
pub struct Quoted<'s> {
    /// Text between the quotes, with escape sequences left as written.
    pub content: Cow<'s, str>,
    /// Position of the opening quote.
    pub start: Cursor,
}

impl Quoted<'_> {
    pub fn content_start(&self) -> Cursor {
        self.start.advanced(['"'])
    }
}

impl Token<'_> {
    /// Position of the token's first character, including the opening quote of a string.
    pub fn cursor(&self) -> Cursor {
        match self {
            Self::Word { start, .. } | Self::Symbol { start, .. } => *start,
            Self::String(literal) => literal.start,
        }
    }
}
