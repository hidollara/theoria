//! Identifiers shared by declarations, references, and attribute parsers.
use crate::Error;

/// Check `[A-Za-z_][A-Za-z0-9_]*`, without resolving names or rejecting reserved words.
const fn is_valid(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.is_empty() || !(bytes[0].is_ascii_alphabetic() || bytes[0] == b'_') {
        return false;
    }
    let mut i = 1;
    while i < bytes.len() {
        if !(bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
            return false;
        }
        i += 1;
    }
    true
}

/// A lexically valid user-defined identifier. Existence and reserved names are checked separately.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Identifier(String);
impl Identifier {
    /// Borrow the validated spelling without checking it again.
    pub fn as_borrowed(&self) -> IdentifierRef<'_> {
        IdentifierRef(&self.0)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn into_string(self) -> String {
        self.0
    }
}
impl std::str::FromStr for Identifier {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self, Error> {
        IdentifierRef::try_from(value).map(IdentifierRef::to_owned)
    }
}
impl TryFrom<String> for Identifier {
    type Error = Error;
    fn try_from(value: String) -> Result<Self, Error> {
        IdentifierRef::try_from(value.as_str())?;
        Ok(Self(value))
    }
}
impl std::ops::Deref for Identifier {
    type Target = str;
    fn deref(&self) -> &str {
        self.as_str()
    }
}
impl AsRef<str> for Identifier {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}
impl std::borrow::Borrow<str> for Identifier {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}
impl std::fmt::Display for Identifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::fmt::Debug for Identifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.0, f)
    }
}
impl From<Identifier> for String {
    fn from(value: Identifier) -> Self {
        value.0
    }
}
impl PartialEq<str> for Identifier {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}
impl PartialEq<&str> for Identifier {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}
impl PartialEq<String> for Identifier {
    fn eq(&self, other: &String) -> bool {
        self.as_str() == other
    }
}

/// A borrowed, lexically valid identifier. Construction is available in const parsers.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IdentifierRef<'a>(&'a str);

impl<'a> IdentifierRef<'a> {
    pub const fn parse(value: &'a str) -> Result<Self, &'static str> {
        if is_valid(value) {
            Ok(Self(value))
        } else {
            Err("invalid identifier")
        }
    }

    pub const fn as_str(self) -> &'a str {
        self.0
    }

    /// Copy the validated spelling into owned storage without checking it again.
    pub fn to_owned(self) -> Identifier {
        Identifier(self.0.to_owned())
    }
}

impl<'a> TryFrom<&'a str> for IdentifierRef<'a> {
    type Error = Error;

    fn try_from(value: &'a str) -> Result<Self, Error> {
        Self::parse(value).map_err(|message| Error(format!("{message} {value:?}")))
    }
}

impl std::ops::Deref for IdentifierRef<'_> {
    type Target = str;

    fn deref(&self) -> &str {
        self.0
    }
}

impl AsRef<str> for IdentifierRef<'_> {
    fn as_ref(&self) -> &str {
        self.0
    }
}

impl std::fmt::Display for IdentifierRef<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

impl std::fmt::Debug for IdentifierRef<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self.0, f)
    }
}
