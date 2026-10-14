//! Output contracts and explicit conformance declarations.
use super::{Error, Port, names, xml};
use std::fmt::Write;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InterfaceDefinition {
    pub name: String,
    pub outputs: Vec<Port>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Implementation {
    pub interface: String,
    pub provides: Vec<Provide>,
}

/// Supply a named output of an Element or an interface implementation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Provide {
    pub name: String,
    pub reference: String,
}

impl Provide {
    pub(super) fn parse(node: roxmltree::Node<'_, '_>) -> Result<Self, Error> {
        xml::check(node, "provide", &["name", "ref"])?;
        xml::arity(node, 0)?;
        let value = Self {
            name: xml::attribute(node, "name")?.into(),
            reference: xml::attribute(node, "ref")?.into(),
        };
        value
            .validate()
            .map_err(|e| xml::error_at(node, &e.to_string()))?;
        Ok(value)
    }

    pub(super) fn validate(&self) -> Result<(), Error> {
        self.name.parse::<names::BindingName>()?;
        self.reference.parse::<names::BindingName>()?;
        Ok(())
    }

    pub(super) fn write(&self, out: &mut String, depth: usize) {
        writeln!(
            out,
            "{}<provide name=\"{}\" ref=\"{}\"/>",
            "  ".repeat(depth),
            xml::escape_attribute(&self.name),
            xml::escape_attribute(&self.reference)
        )
        .unwrap();
    }
}

impl InterfaceDefinition {
    pub(super) fn parse(node: roxmltree::Node<'_, '_>) -> Result<Self, Error> {
        xml::check(node, "interface", &["name"])?;
        let mut outputs = Vec::new();
        for child in node.children().filter(roxmltree::Node::is_element) {
            xml::check(child, "output", &["name", "type", "fallible"])?;
            xml::arity(child, 0)?;
            outputs.push(Port {
                name: xml::attribute(child, "name")?.into(),
                ty: xml::value_type(child)?,
            });
        }
        let value = Self {
            name: xml::attribute(node, "name")?.into(),
            outputs,
        };
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<(), Error> {
        if !names::identifier(&self.name) || super::reserved_name(&self.name) {
            return Err(Error("invalid interface name".into()));
        }
        for output in &self.outputs {
            output.name.parse::<names::BindingName>()?;
            output.ty.kind.validate()?;
        }
        Ok(())
    }
    pub(super) fn write(&self, out: &mut String) {
        writeln!(
            out,
            "  <interface name=\"{}\">",
            xml::escape_attribute(&self.name)
        )
        .unwrap();
        for port in &self.outputs {
            writeln!(
                out,
                "    <output name=\"{}\" type=\"{}\"{}/>",
                xml::escape_attribute(&port.name),
                xml::escape_attribute(port.ty.kind.spelling()),
                if port.ty.fallible {
                    " fallible=\"true\""
                } else {
                    ""
                }
            )
            .unwrap();
        }
        out.push_str("  </interface>\n");
    }
}

impl Implementation {
    pub(super) fn parse(node: roxmltree::Node<'_, '_>) -> Result<Self, Error> {
        xml::check(node, "impl", &["interface"])?;
        let mut provides = Vec::new();
        for child in node.children().filter(roxmltree::Node::is_element) {
            provides.push(Provide::parse(child)?);
        }
        let value = Self {
            interface: xml::attribute(node, "interface")?.into(),
            provides,
        };
        value.validate()?;
        Ok(value)
    }
    pub(super) fn validate(&self) -> Result<(), Error> {
        self.interface.parse::<names::BindingName>()?;
        for value in &self.provides {
            value.validate()?;
        }
        Ok(())
    }
    pub(super) fn write(&self, out: &mut String, depth: usize) {
        let pad = "  ".repeat(depth);
        writeln!(
            out,
            "{pad}<impl interface=\"{}\">",
            xml::escape_attribute(&self.interface)
        )
        .unwrap();
        for value in &self.provides {
            value.write(out, depth + 1);
        }
        writeln!(out, "{pad}</impl>").unwrap();
    }
}
