//! Template attribute grammar used by the Theoria string implementation.
use super::{ParseBuffer, ParseContext, ParseError};

pub(super) const fn parse<'a, const N: usize>(
    source: &'a str,
    cx: &mut ParseContext<'_>,
    parts: &mut ParseBuffer<'a, TextPart<'a>, N>,
) -> Result<(), ParseError> {
    let mut scanner = Scanner::new(source);
    loop {
        let part = match scanner.next_part() {
            Ok(Some(part)) => part,
            Ok(None) => return Ok(()),
            Err(error) => return Err(ParseError::new(error)),
        };
        if let TextPart::Reference(name) = part
            && let Err(error) = cx.reference::<crate::Display>(name)
        {
            return Err(error);
        }
        if let Err(error) = parts.push(part) {
            return Err(error);
        }
    }
}

/// Borrowed template fragments stored by the string parser, not a common parser IR.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextPart<'a> {
    Literal(&'a str),
    Reference(&'a str),
}

struct Scanner<'a> {
    source: &'a str,
    position: usize,
}

impl<'a> Scanner<'a> {
    pub const fn new(source: &'a str) -> Self {
        Self {
            source,
            position: 0,
        }
    }

    pub const fn next_part(&mut self) -> Result<Option<TextPart<'a>>, &'static str> {
        let bytes = self.source.as_bytes();
        let start = self.position;
        if start == bytes.len() {
            return Ok(None);
        }
        if bytes[start] != b'{' && bytes[start] != b'}' {
            while self.position < bytes.len()
                && bytes[self.position] != b'{'
                && bytes[self.position] != b'}'
            {
                self.position += 1;
            }
            return Ok(Some(TextPart::Literal(slice(
                self.source,
                start,
                self.position,
            ))));
        }
        if start + 1 < bytes.len() && bytes[start] == bytes[start + 1] {
            self.position += 2;
            return Ok(Some(TextPart::Literal(slice(
                self.source,
                start,
                start + 1,
            ))));
        }
        if bytes[start] == b'}' {
            return Err("unescaped closing brace");
        }
        self.position += 1;
        while self.position < bytes.len() && bytes[self.position] != b'}' {
            self.position += 1;
        }
        if self.position == bytes.len() {
            return Err("unclosed attribute reference");
        }
        let name = slice(self.source, start + 1, self.position);
        self.position += 1;
        if !organon_core::identifier::is_valid(name) {
            return Err("invalid attribute reference name");
        }
        Ok(Some(TextPart::Reference(name)))
    }
}

// All boundaries are the source endpoints or adjacent to an ASCII brace.
const fn slice(source: &str, start: usize, end: usize) -> &str {
    source.split_at(end).0.split_at(start).1
}

#[cfg(test)]
mod tests {
    use super::{Scanner, TextPart};

    fn scan(source: &str) -> Result<Vec<TextPart<'_>>, &'static str> {
        let mut scanner = Scanner {
            source,
            position: 0,
        };
        let mut parts = vec![];
        while let Some(part) = scanner.next_part()? {
            parts.push(part);
        }
        Ok(parts)
    }

    #[test]
    fn literals_escaped_braces_and_multiple_references() {
        assert_eq!(
            scan("{{{left}}} + {right}").unwrap(),
            vec![
                TextPart::Literal("{"),
                TextPart::Reference("left"),
                TextPart::Literal("}"),
                TextPart::Literal(" + "),
                TextPart::Reference("right"),
            ]
        );
        assert!(scan("").unwrap().is_empty());
        assert_eq!(
            scan("日本語\n\t🦀{{}}").unwrap(),
            vec![
                TextPart::Literal("日本語\n\t🦀"),
                TextPart::Literal("{"),
                TextPart::Literal("}"),
            ]
        );
    }

    #[test]
    fn malformed_templates_are_rejected_by_the_library_parser() {
        for text in ["{", "}", "{}", "{a.b}", "{a{b}", "{日本語}", "{0}"] {
            assert!(scan(text).is_err(), "{text}");
        }
    }
}
