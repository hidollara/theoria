use super::{Error, names, xml};
use std::fmt::Write;

/// Publish a local or imported definition from an Organon file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Export {
    pub name: String,
    pub alias: Option<String>,
}

impl Export {
    pub fn public_name(&self) -> &str {
        self.alias.as_deref().unwrap_or(&self.name)
    }

    pub(super) fn validate(&self) -> Result<(), Error> {
        if !names::identifier(&self.name) || !names::identifier(self.public_name()) {
            return Err(Error("invalid public name".into()));
        }
        Ok(())
    }

    pub(super) fn parse(node: roxmltree::Node<'_, '_>) -> Result<Self, Error> {
        xml::check(node, "export", &["name", "as"])?;
        xml::arity(node, 0)?;
        let value = Self {
            name: xml::attribute(node, "name")?.into(),
            alias: node.attribute("as").map(str::to_owned),
        };
        value
            .validate()
            .map_err(|e| xml::error_at(node, &e.to_string()))?;
        Ok(value)
    }

    pub(super) fn write(&self, text: &mut String) {
        write!(text, "  <export name=\"{}\"", self.name).unwrap();
        if let Some(alias) = &self.alias {
            write!(text, " as=\"{}\"", alias).unwrap();
        }
        text.push_str("/>\n");
    }
}
