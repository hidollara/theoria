//! File-level declarations and contracts implemented in Rust.
use crate::{ElementDefinition, Error, InputType, template, xml};
use std::{collections::HashSet, fmt::Write};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefinitionFile {
    /// File paths relative to this file, resolved by the compiler.
    pub imports: Vec<String>,
    pub definitions: Vec<Definition>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Definition {
    Element(ElementDefinition),
    Interface(InterfaceDefinition),
}

impl Definition {
    pub fn name(&self) -> &str {
        match self {
            Self::Element(v) => &v.name,
            Self::Interface(v) => &v.name,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InterfaceDefinition {
    pub name: String,
    pub inputs: Vec<Port>,
    pub outputs: Vec<InterfaceOutput>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Port {
    pub name: String,
    pub ty: PortType,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortType {
    pub kind: InputType,
    /// Result<T, &'static str>, independent from reactive evaluation policy.
    pub fallible: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InterfaceOutput {
    pub port: Port,
    pub inline: bool,
}

impl PortType {
    pub fn parse(value: &str) -> Result<Self, Error> {
        let (value, fallible) = match value
            .strip_prefix("Result<")
            .and_then(|v| v.strip_suffix('>'))
        {
            Some(value) => (value, true),
            None => (value, false),
        };
        let kind = InputType::parse(value)?;
        Ok(Self { kind, fallible })
    }

    pub fn spelling(&self) -> String {
        let value = self.kind.spelling();
        if self.fallible {
            format!("Result<{value}>")
        } else {
            value.into()
        }
    }
}

impl InterfaceDefinition {
    pub fn validate(&self) -> Result<(), Error> {
        if !template::identifier(&self.name) || crate::primitive_name(&self.name) {
            return Err(Error("invalid or reserved interface name".into()));
        }
        for port in self
            .inputs
            .iter()
            .chain(self.outputs.iter().map(|v| &v.port))
        {
            port.ty.kind.validate()?;
        }
        for names in [
            self.inputs.iter().map(|v| &v.name).collect::<Vec<_>>(),
            self.outputs.iter().map(|v| &v.port.name).collect(),
        ] {
            let mut seen = HashSet::new();
            for name in names {
                if !template::identifier(name) || !seen.insert(name) {
                    return Err(Error(format!("invalid or duplicate port {name:?}")));
                }
            }
        }
        if self.outputs.is_empty() {
            return Err(Error("interface requires at least one output".into()));
        }
        Ok(())
    }

    fn parse(node: roxmltree::Node<'_, '_>) -> Result<Self, Error> {
        xml::check(node, "interface", &["name"])?;
        let mut value = Self {
            name: xml::attribute(node, "name")?.into(),
            inputs: vec![],
            outputs: vec![],
        };
        for child in node.children().filter(roxmltree::Node::is_element) {
            let output = child.tag_name().name() == "output";
            xml::check(
                child,
                if output { "output" } else { "input" },
                if output {
                    &["name", "type", "inline"]
                } else {
                    &["name", "type"]
                },
            )?;
            xml::arity(child, 0)?;
            let port = Port {
                name: xml::attribute(child, "name")?.into(),
                ty: PortType::parse(xml::attribute(child, "type")?)
                    .map_err(|e| xml::at(child, &e.to_string()))?,
            };
            if output {
                let inline = match child.attribute("inline") {
                    None | Some("false") => false,
                    Some("true") => true,
                    _ => return Err(xml::at(child, "inline requires a fixed true or false")),
                };
                value.outputs.push(InterfaceOutput { port, inline });
            } else {
                value.inputs.push(port);
            }
        }
        value
            .validate()
            .map_err(|e| xml::at(node, &e.to_string()))?;
        Ok(value)
    }
}

impl DefinitionFile {
    pub fn from_xml(source: &str) -> Result<Self, Error> {
        xml::check_source_depth(source)?;
        let document = roxmltree::Document::parse(source).map_err(|e| Error(e.to_string()))?;
        let root = document.root_element();
        // Preserve the initial single-Element API while introducing file-level declarations.
        if root.tag_name().name() == "element" {
            return Ok(Self {
                imports: vec![],
                definitions: vec![Definition::Element(ElementDefinition::from_xml(source)?)],
            });
        }
        xml::check(root, "organon", &["version"])?;
        if xml::attribute(root, "version")? != "1" {
            return Err(xml::at(root, "unsupported organon version (expected 1)"));
        }
        let mut definitions = vec![];
        let mut imports = vec![];
        let mut seen = HashSet::new();
        for node in root.children().filter(roxmltree::Node::is_element) {
            if node.tag_name().name() == "import" {
                xml::check(node, "import", &["path"])?;
                xml::arity(node, 0)?;
                let path = xml::attribute(node, "path")?;
                if path.trim().is_empty() || path.contains('\0') {
                    return Err(xml::at(node, "import requires a nonempty file path"));
                }
                imports.push(path.to_owned());
                continue;
            }
            let definition = match node.tag_name().name() {
                "interface" => Definition::Interface(InterfaceDefinition::parse(node)?),
                "element" => Definition::Element(ElementDefinition::from_node(node)?),
                _ => return Err(xml::at(node, "expected import, element or interface")),
            };
            if !seen.insert(definition.name().to_owned()) {
                return Err(xml::at(node, "duplicate definition name"));
            }
            definitions.push(definition);
        }
        if definitions.is_empty() && imports.is_empty() {
            return Err(xml::at(root, "organon requires at least one definition"));
        }
        Ok(Self {
            imports,
            definitions,
        })
    }

    pub fn to_xml(&self) -> Result<String, Error> {
        let mut text = String::from("<organon version=\"1\">\n");
        for path in &self.imports {
            writeln!(text, "  <import path=\"{}\"/>", xml::escape_attribute(path)).unwrap();
        }
        for definition in &self.definitions {
            match definition {
                Definition::Element(value) => {
                    let xml = value.to_xml()?.replacen(" version=\"1\"", "", 1);
                    for line in xml.lines() {
                        writeln!(text, "  {line}").unwrap();
                    }
                }
                Definition::Interface(value) => {
                    value.validate()?;
                    writeln!(
                        text,
                        "  <interface name=\"{}\">",
                        xml::escape_attribute(&value.name)
                    )
                    .unwrap();
                    for port in &value.inputs {
                        write_port(&mut text, "input", port, false);
                    }
                    for output in &value.outputs {
                        write_port(&mut text, "output", &output.port, output.inline);
                    }
                    text.push_str("  </interface>\n");
                }
            }
        }
        text.push_str("</organon>\n");
        Self::from_xml(&text)?;
        Ok(text)
    }
}

fn write_port(text: &mut String, tag: &str, port: &Port, inline: bool) {
    writeln!(
        text,
        "    <{tag} name=\"{}\" type=\"{}\"{}/>",
        xml::escape_attribute(&port.name),
        xml::escape_attribute(&port.ty.spelling()),
        if inline { " inline=\"true\"" } else { "" }
    )
    .unwrap();
}
