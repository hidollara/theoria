use super::{Error, names, xml};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportSource {
    Relative(String),
    Crate(String),
}
impl ImportSource {
    pub fn parse(value: &str) -> Result<Self, Error> {
        if value.starts_with("./") || value.starts_with("../") {
            if value.contains('\\') || value.contains('\0') || value.contains(':') {
                return Err(Error(
                    "relative import paths must use forward slashes and no drive prefix".into(),
                ));
            }
            xml::text(value)?;
            return Ok(Self::Relative(value.into()));
        }
        if !value.is_empty()
            && value
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
            && value.as_bytes()[0].is_ascii_alphabetic()
        {
            return Ok(Self::Crate(value.into()));
        }
        Err(Error(
            "from must be a dependency name or a relative path beginning with ./ or ../".into(),
        ))
    }
    pub fn as_str(&self) -> &str {
        match self {
            Self::Relative(s) | Self::Crate(s) => s,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
/// Import all public names from one source without republishing them.
pub struct Import {
    pub from: ImportSource,
}
impl Import {
    pub fn validate(&self) -> Result<(), Error> {
        self.from.validate()
    }
    pub(super) fn parse(node: roxmltree::Node<'_, '_>) -> Result<Self, Error> {
        xml::check(node, "import", &["from"])?;
        xml::arity(node, 0)?;
        Ok(Self {
            from: ImportSource::parse(xml::attribute(node, "from")?)?,
        })
    }
    pub(super) fn write(&self, text: &mut String) {
        use std::fmt::Write;
        writeln!(
            text,
            "  <import from=\"{}\"/>",
            xml::escape_attribute(self.from.as_str())
        )
        .unwrap();
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Select one public name from a source, optionally under a local alias.
pub struct Use {
    pub from: ImportSource,
    pub name: String,
    pub alias: Option<String>,
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
        self.from.validate()?;
        for name in std::iter::once(&self.name).chain(self.alias.iter()) {
            if !names::identifier(name) {
                return Err(Error(format!("invalid imported name {name:?}")));
            }
        }
        Ok(())
    }
    pub(super) fn parse(node: roxmltree::Node<'_, '_>) -> Result<Self, Error> {
        xml::check(node, "use", &["from", "name", "as"])?;
        xml::arity(node, 0)?;
        let value = Self {
            from: ImportSource::parse(xml::attribute(node, "from")?)?,
            name: xml::attribute(node, "name")?.into(),
            alias: node.attribute("as").map(str::to_owned),
        };
        value.validate()?;
        Ok(value)
    }
    pub(super) fn write(&self, text: &mut String) {
        use std::fmt::Write;
        write!(
            text,
            "  <use from=\"{}\" name=\"{}\"",
            xml::escape_attribute(self.from.as_str()),
            self.name
        )
        .unwrap();
        if let Some(alias) = &self.alias {
            write!(text, " as=\"{}\"", alias).unwrap();
        }
        text.push_str("/>\n");
    }
}
