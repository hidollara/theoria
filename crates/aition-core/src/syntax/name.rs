//! Identifiers and qualified names in source syntax.

use std::{error::Error, fmt, str::FromStr};

use crate::lexer::Symbol;

use super::parser::{
    Expected, Parsable, Parser,
    grammar::{rule, symbol, word},
    node::AstNode,
};

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

/// A name written as one or more dot-separated identifiers, such as `Panel` or
/// `panels.Panel`. Name resolution determines what each identifier refers to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QualifiedName(Box<[AstNode<Identifier>]>);

impl QualifiedName {
    /// The required head guarantees that the stored sequence is nonempty.
    pub fn new(
        head: AstNode<Identifier>,
        tail: impl IntoIterator<Item = AstNode<Identifier>>,
    ) -> Self {
        Self(std::iter::once(head).chain(tail).collect())
    }

    pub fn as_slice(&self) -> &[AstNode<Identifier>] {
        &self.0
    }
}

impl Parsable for Identifier {
    fn parser() -> impl Parser<Output = Self> {
        word(Expected::Identifier).try_map(Expected::Identifier, |text| Identifier::new(text).ok())
    }
}

impl Parsable for QualifiedName {
    fn parser() -> impl Parser<Output = Self> {
        rule::<AstNode<Identifier>>()
            .then(
                symbol(Symbol::Dot)
                    .then(rule::<AstNode<Identifier>>())
                    .map(|(_, name)| name)
                    .repeated(),
            )
            .map(|(head, tail)| QualifiedName::new(head, tail))
    }
}

#[cfg(test)]
mod tests {
    use crate::syntax::name::{Identifier, QualifiedName};

    #[test]
    fn accepts_identifier_spellings() {
        for text in ["Panel", "drawingOf", "_", "_width", "width2", "read_error"] {
            let identifier: Identifier = text.parse().unwrap();
            assert_eq!(identifier.as_str(), text);
        }
    }

    #[test]
    fn rejects_empty_names_digits_at_start_and_other_characters() {
        for text in [
            "",
            "2width",
            "panel.width",
            "panel-width",
            "width ",
            "幅",
            "aé",
        ] {
            assert!(Identifier::new(text).is_err(), "{text:?}");
        }
    }

    #[test]
    fn qualified_names_accept_a_head_without_a_tail() {
        let name = QualifiedName::new(Identifier::new("Panel").unwrap().into(), []);
        assert_eq!(name.as_slice().len(), 1);
        assert_eq!(name.as_slice()[0].value.as_str(), "Panel");
    }

    #[test]
    fn qualified_names_expose_identifiers_in_source_order() {
        let head = Identifier::new("panels").unwrap();
        let tail = ["nested", "Panel"].map(|text| Identifier::new(text).unwrap().into());
        let name = QualifiedName::new(head.into(), tail);
        let spellings: Vec<_> = name
            .as_slice()
            .iter()
            .map(|node| node.value.as_str())
            .collect();
        assert_eq!(spellings, ["panels", "nested", "Panel"]);
    }
}
