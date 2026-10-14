//! Identifiers shared by declarations, references, and attribute parsers.
use crate::Error;

pub fn identifier(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// A binding name, parsed without looking up a binding environment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BindingName(String);
impl BindingName {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl std::str::FromStr for BindingName {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if identifier(value) {
            Ok(Self(value.into()))
        } else {
            Err(Error(
                "expected a binding name without braces or dots".into(),
            ))
        }
    }
}

impl std::fmt::Display for BindingName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
