//! Import all published names or select one name from another source scope.
use crate::Error;
use crate::identifier::Identifier;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportSource {
    Relative(String),
    Dependency(String),
}
impl ImportSource {
    pub fn parse(value: &str) -> Result<Self, Error> {
        if value.starts_with("./") || value.starts_with("../") {
            if value.contains('\\') || value.contains('\0') || value.contains(':') {
                return Err(Error(
                    "relative import paths must use forward slashes and no drive prefix".into(),
                ));
            }
            return Ok(Self::Relative(value.into()));
        }
        if !value.is_empty()
            && value
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
            && value.as_bytes()[0].is_ascii_alphabetic()
        {
            return Ok(Self::Dependency(value.into()));
        }
        Err(Error(
            "from must be a dependency name or a relative path beginning with ./ or ../".into(),
        ))
    }
    pub fn as_str(&self) -> &str {
        match self {
            Self::Relative(s) | Self::Dependency(s) => s,
        }
    }
}
/// Import all public names from one source without republishing them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Import {
    pub from: ImportSource,
}
impl Import {
    pub fn validate(&self) -> Result<(), Error> {
        self.from.validate()
    }
}

/// Select one public name from a source, optionally under a local alias.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Use {
    pub from: ImportSource,
    pub name: Identifier,
    pub alias: Option<Identifier>,
}
impl ImportSource {
    fn validate(&self) -> Result<(), Error> {
        if Self::parse(self.as_str())? != *self {
            return Err(Error("invalid import source kind".into()));
        }
        Ok(())
    }
}
impl Use {
    pub fn validate(&self) -> Result<(), Error> {
        self.from.validate()
    }
}
