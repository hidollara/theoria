use std::{error::Error, fmt, str::FromStr};

/// An identifier with the spelling `[A-Za-z_][A-Za-z0-9_]*`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identifier(Box<str>);

impl Identifier {
    pub fn new(text: impl Into<Box<str>>) -> Result<Self, InvalidIdentifier> {
        let text = text.into();
        let mut bytes = text.bytes();
        let valid_start = bytes
            .next()
            .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_');
        let valid_rest = bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');

        if valid_start && valid_rest {
            Ok(Self(text))
        } else {
            Err(InvalidIdentifier(text))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
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
pub struct InvalidIdentifier(Box<str>);

impl fmt::Display for InvalidIdentifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid identifier {:?}", self.0)
    }
}

impl Error for InvalidIdentifier {}
