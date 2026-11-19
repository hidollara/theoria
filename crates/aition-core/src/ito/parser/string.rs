use super::super::lexer::TokenKind;
use super::{ParseError, Parser, Rule};
use crate::input::Cursor;
use std::str::Chars;

/// Read quoted text, decoding escapes without interpreting the literal's target type.
/// Supports `\"`, `\\`, `\n`, `\r`, `\t`, and `\u{H...}` with one to six hex digits
/// naming a Unicode scalar value. Invalid escapes report their position in the source.
pub fn string() -> impl Rule<Output = (Box<str>, Cursor)> {
    StringRule
}

struct StringRule;

impl Rule for StringRule {
    type Output = (Box<str>, Cursor);

    fn description(&self) -> String {
        "a string literal".into()
    }

    fn read(&self, parser: &mut Parser<'_, '_>) -> Result<Option<Self::Output>, ParseError> {
        let Some(token) = parser.next_if(|kind| matches!(kind, TokenKind::String(_)))? else {
            return Ok(None);
        };
        let start = token.content_start();
        let TokenKind::String(text) = token.kind else {
            unreachable!()
        };
        let text = decode(&text, start)?;
        Ok(Some((text, token.end)))
    }
}

fn decode(text: &str, mut cursor: Cursor) -> Result<Box<str>, ParseError> {
    let mut decoded = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(ch) = next(&mut chars, &mut cursor) {
        decoded.push(if ch == '\\' {
            let start = cursor;
            match next(&mut chars, &mut cursor) {
                Some('"') => '"',
                Some('\\') => '\\',
                Some('n') => '\n',
                Some('r') => '\r',
                Some('t') => '\t',
                Some('u') => unicode(&mut chars, &mut cursor)?,
                _ => {
                    return Err(ParseError::expected(
                        "a valid escape character".into(),
                        start,
                    ));
                }
            }
        } else {
            ch
        });
    }
    Ok(decoded.into_boxed_str())
}

fn next(chars: &mut Chars<'_>, cursor: &mut Cursor) -> Option<char> {
    let ch = chars.next()?;
    cursor.advance(ch);
    Some(ch)
}

fn unicode(chars: &mut Chars<'_>, cursor: &mut Cursor) -> Result<char, ParseError> {
    let start = *cursor;
    if next(chars, cursor) != Some('{') {
        return Err(ParseError::expected("'{'".into(), start));
    }
    let value_start = *cursor;
    let mut value = 0;
    let mut digits = 0;
    loop {
        let start = *cursor;
        match next(chars, cursor) {
            Some('}') if digits > 0 => {
                return char::from_u32(value).ok_or_else(|| {
                    ParseError::expected("a Unicode scalar value".into(), value_start)
                });
            }
            Some(ch) if digits < 6 && ch.is_ascii_hexdigit() => {
                value = value * 16 + ch.to_digit(16).unwrap();
                digits += 1;
            }
            _ => {
                let expected = match digits {
                    0 => "a hexadecimal digit",
                    6 => "'}'",
                    _ => "a hexadecimal digit or '}'",
                };
                return Err(ParseError::expected(expected.into(), start));
            }
        }
    }
}

#[cfg(test)]
mod tests;
