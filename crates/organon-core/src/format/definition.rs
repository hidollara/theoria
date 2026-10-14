//! File-level declarations and contracts implemented in Rust.
use super::{ElementDefinition, Error, ExternalDefinition, xml};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefinitionFile {
    pub declarations: Vec<FileDeclaration>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileDeclaration {
    /// Import all public names from a file-relative or package source.
    Import(super::Import),
    /// Select one public name, optionally under a local alias.
    Use(super::Use),
    /// Publish a local or imported definition from this file.
    Export(super::Export),
    Definition(Definition),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Definition {
    Interface(super::InterfaceDefinition),
    Element(ElementDefinition),
    External(ExternalDefinition),
}

impl Definition {
    pub fn name(&self) -> &str {
        match self {
            Self::Element(v) => &v.name,
            Self::Interface(v) => &v.name,
            Self::External(v) => &v.name,
        }
    }
}

impl DefinitionFile {
    pub fn from_xml(source: &str) -> Result<Self, Error> {
        let document = roxmltree::Document::parse(source).map_err(|e| Error(e.to_string()))?;
        let root = document.root_element();
        xml::check(root, "organon", &["version"])?;
        if xml::attribute(root, "version")? != "1" {
            return Err(xml::error_at(
                root,
                "unsupported organon version (expected 1)",
            ));
        }
        let mut declarations = vec![];
        for node in root.children().filter(roxmltree::Node::is_element) {
            if node.tag_name().name() == "import" {
                declarations.push(FileDeclaration::Import(super::Import::parse(node)?));
                continue;
            }
            if node.tag_name().name() == "export" {
                declarations.push(FileDeclaration::Export(super::Export::parse(node)?));
                continue;
            }
            if node.tag_name().name() == "use" {
                declarations.push(FileDeclaration::Use(super::Use::parse(node)?));
                continue;
            }
            let definition = match node.tag_name().name() {
                "interface" => Definition::Interface(super::InterfaceDefinition::parse(node)?),
                "declare" => Definition::External(ExternalDefinition::parse(node)?),
                "element" => Definition::Element(ElementDefinition::from_node(node)?),
                _ => {
                    return Err(xml::error_at(
                        node,
                        "expected import, use, export, element, declare, or interface",
                    ));
                }
            };
            declarations.push(FileDeclaration::Definition(definition));
        }
        Ok(Self { declarations })
    }

    pub fn imports(&self) -> impl Iterator<Item = &super::Import> {
        self.declarations.iter().filter_map(|d| match d {
            FileDeclaration::Import(v) => Some(v),
            _ => None,
        })
    }
    pub fn definitions(&self) -> impl Iterator<Item = &Definition> {
        self.declarations.iter().filter_map(|d| match d {
            FileDeclaration::Definition(v) => Some(v),
            _ => None,
        })
    }
    pub fn uses(&self) -> impl Iterator<Item = &super::Use> {
        self.declarations.iter().filter_map(|d| match d {
            FileDeclaration::Use(v) => Some(v),
            _ => None,
        })
    }
    pub fn sources(&self) -> impl Iterator<Item = &super::ImportSource> {
        self.declarations.iter().filter_map(|d| match d {
            FileDeclaration::Import(v) => Some(&v.from),
            FileDeclaration::Use(v) => Some(&v.from),
            _ => None,
        })
    }
    pub fn validate(&self) -> Result<(), Error> {
        for declaration in &self.declarations {
            match declaration {
                FileDeclaration::Import(value) => value.validate()?,
                FileDeclaration::Use(value) => value.validate()?,
                FileDeclaration::Export(value) => value.validate()?,
                FileDeclaration::Definition(Definition::Interface(value)) => value.validate()?,
                FileDeclaration::Definition(Definition::Element(value)) => value.validate()?,
                FileDeclaration::Definition(Definition::External(value)) => value.validate()?,
            }
        }
        Ok(())
    }

    pub fn to_xml(&self) -> Result<String, Error> {
        self.validate()?;
        let mut text = String::from("<organon version=\"1\">\n");
        for declaration in &self.declarations {
            let definition = match declaration {
                FileDeclaration::Import(value) => {
                    value.write(&mut text);
                    continue;
                }
                FileDeclaration::Use(value) => {
                    value.write(&mut text);
                    continue;
                }
                FileDeclaration::Export(value) => {
                    value.write(&mut text);
                    continue;
                }
                FileDeclaration::Definition(value) => value,
            };
            match definition {
                Definition::Interface(value) => value.write(&mut text),
                Definition::Element(value) => {
                    value.write(&mut text);
                }
                Definition::External(value) => {
                    value.write(&mut text);
                }
            }
        }
        text.push_str("</organon>\n");
        Ok(text)
    }
}
