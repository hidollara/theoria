use super::super::error::ParseError;
use super::super::rule::{Rule, quoted_text};
use crate::ito::Cursor;
use crate::syntax::Expr;

pub(super) fn literal() -> Rule<Expr> {
    quoted_text()
        .try_map(|(text, cursor)| decode_string(&text, cursor))
        .map(Expr::Literal)
}

fn decode_string(text: &str, start: Cursor) -> Result<Box<str>, ParseError> {
    let mut decoded = String::with_capacity(text.len());
    let mut chars = text.char_indices();
    while let Some((offset, ch)) = chars.next() {
        decoded.push(if ch == '\\' {
            match chars.next().map(|(_, ch)| ch) {
                Some('"') => '"',
                Some('\\') => '\\',
                Some('n') => '\n',
                Some('r') => '\r',
                Some('t') => '\t',
                _ => {
                    return Err(ParseError::expected(
                        "a valid string escape",
                        start.advanced(text[..offset].chars()),
                    ));
                }
            }
        } else {
            ch
        });
    }
    Ok(decoded.into_boxed_str())
}
