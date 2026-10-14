//! Identifiers shared by declarations, references, and attribute parsers.
use crate::Error;

/// Check `[A-Za-z_][A-Za-z0-9_]*`, without resolving names or rejecting reserved words.
pub const fn is_valid(name: &str) -> bool {
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
        Self::try_from(value.to_owned())
    }
}
impl TryFrom<String> for Identifier {
    type Error = Error;
    fn try_from(value: String) -> Result<Self, Error> {
        if is_valid(&value) {
            Ok(Self(value))
        } else {
            Err(Error(format!("invalid identifier {value:?}")))
        }
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
