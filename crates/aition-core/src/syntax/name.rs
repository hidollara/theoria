use crate::ito::Cursor;
use std::{error::Error, fmt, str::FromStr};

/// An identifier with the spelling `[A-Za-z_][A-Za-z0-9_]*`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identifier {
    text: Box<str>,
    /// Source position, when this identifier came from a file.
    pub cursor: Option<Cursor>,
}

impl Identifier {
    pub fn new(text: impl Into<Box<str>>) -> Result<Self, InvalidIdentifier> {
        let text = text.into();
        let mut bytes = text.bytes();
        let valid_start = bytes
            .next()
            .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_');
        if !valid_start {
            return Err(InvalidIdentifier {
                text,
                expected: "an identifier starting with an ASCII letter or '_'",
            });
        }
        if !bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_') {
            return Err(InvalidIdentifier {
                text,
                expected: "an identifier containing only ASCII letters, digits, or '_'",
            });
        }
        Ok(Self { text, cursor: None })
    }

    pub fn with_cursor(mut self, cursor: Cursor) -> Self {
        self.cursor = Some(cursor);
        self
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }
}

impl FromStr for Identifier {
    type Err = InvalidIdentifier;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::new(text)
    }
}

impl fmt::Display for Identifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvalidIdentifier {
    text: Box<str>,
    pub(crate) expected: &'static str,
}

impl fmt::Display for InvalidIdentifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "expected {}, found {:?}",
            self.expected, self.text
        )
    }
}

impl Error for InvalidIdentifier {}

/// One or more dot-separated identifiers, in source order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QualifiedName(Box<[Identifier]>);

impl QualifiedName {
    pub fn new(head: Identifier, tail: impl IntoIterator<Item = Identifier>) -> Self {
        Self(std::iter::once(head).chain(tail).collect())
    }

    pub fn as_slice(&self) -> &[Identifier] {
        &self.0
    }
}
