//! External construction declarations and their XML representation.
use super::{Error, Port, Type, names, xml};
use crate::types::Children;
use std::fmt::Write;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalDefinition {
    pub name: String,
    pub tag: bool,
    pub declarations: Vec<ExternalDeclaration>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExternalDeclaration {
    Implements(String),
    Input(Port),
    OptionalInput(Port),
    Children(Children),
    Impl(super::Implementation),
    Output(DeclaredOutput),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeclaredOutput {
    pub port: Port,
    pub inline: bool,
}

impl ExternalDefinition {
    pub fn validate(&self) -> Result<(), Error> {
        if !names::identifier(&self.name) || crate::format::reserved_name(&self.name) {
            return Err(Error("invalid or reserved declaration name".into()));
        }
        if self
            .declarations
            .iter()
            .filter(|d| matches!(d, ExternalDeclaration::Children(_)))
            .count()
            > 1
        {
            return Err(Error("duplicate children contract".into()));
        }
        for d in &self.declarations {
            match d {
                ExternalDeclaration::Children(c) => c.validate()?,
                ExternalDeclaration::Impl(i) => i.validate()?,
                _ => (),
            }
        }
        for name in self.implements() {
            name.parse::<names::BindingName>()?;
        }
        for port in self.inputs().chain(self.outputs().map(|v| &v.port)) {
            port.ty.kind.validate()?;
            if !names::identifier(&port.name) {
                return Err(Error(format!("invalid port {:?}", port.name)));
            }
        }
        Ok(())
    }

    pub fn implements(&self) -> impl Iterator<Item = &str> {
        self.declarations.iter().filter_map(|d| match d {
            ExternalDeclaration::Implements(n) => Some(n.as_str()),
            ExternalDeclaration::Impl(i) => Some(i.interface.as_str()),
            _ => None,
        })
    }
    pub fn inputs(&self) -> impl Iterator<Item = &Port> {
        self.declarations.iter().filter_map(|d| match d {
            ExternalDeclaration::Input(v) | ExternalDeclaration::OptionalInput(v) => Some(v),
            _ => None,
        })
    }
    pub fn outputs(&self) -> impl Iterator<Item = &DeclaredOutput> {
        self.declarations.iter().filter_map(|d| match d {
            ExternalDeclaration::Output(v) => Some(v),
            _ => None,
        })
    }

    pub fn optional(&self, name: &str) -> bool {
        self.declarations
            .iter()
            .any(|d| matches!(d, ExternalDeclaration::OptionalInput(p) if p.name == name))
    }
    pub fn children(&self) -> Option<&Children> {
        self.declarations.iter().find_map(|d| match d {
            ExternalDeclaration::Children(c) => Some(c),
            _ => None,
        })
    }
    pub fn implementation(&self, name: &str) -> Option<&super::Implementation> {
        self.declarations.iter().find_map(|d| match d {
            ExternalDeclaration::Impl(i) if i.interface == name => Some(i),
            _ => None,
        })
    }
    pub(super) fn parse(node: roxmltree::Node<'_, '_>) -> Result<Self, Error> {
        xml::check(node, "declare", &["name", "tag"])?;
        let mut value = Self {
            name: xml::attribute(node, "name")?.into(),
            tag: xml::boolean(node, "tag", true)?,
            declarations: vec![],
        };
        for child in node.children().filter(roxmltree::Node::is_element) {
            if child.tag_name().name() == "implements" {
                xml::check(child, "implements", &["interface"])?;
                xml::arity(child, 0)?;
                value.declarations.push(ExternalDeclaration::Implements(
                    xml::attribute(child, "interface")?.into(),
                ));
                continue;
            }
            if child.tag_name().name() == "children" {
                value
                    .declarations
                    .push(ExternalDeclaration::Children(parse_children(child)?));
                continue;
            }
            if child.tag_name().name() == "impl" {
                value
                    .declarations
                    .push(ExternalDeclaration::Impl(super::Implementation::parse(
                        child,
                    )?));
                continue;
            }
            let output = child.tag_name().name() == "output";
            xml::check(
                child,
                if output { "output" } else { "input" },
                if output {
                    &["name", "type", "inline", "fallible"]
                } else {
                    &["name", "type", "optional", "fallible"]
                },
            )?;
            xml::arity(child, 0)?;
            let port = Port {
                name: xml::attribute(child, "name")?.into(),
                ty: xml::value_type(child)?,
            };
            if output {
                let inline = xml::boolean(child, "inline", false)?;
                value
                    .declarations
                    .push(ExternalDeclaration::Output(DeclaredOutput { port, inline }));
            } else {
                value
                    .declarations
                    .push(if xml::boolean(child, "optional", false)? {
                        ExternalDeclaration::OptionalInput(port)
                    } else {
                        ExternalDeclaration::Input(port)
                    });
            }
        }
        value
            .validate()
            .map_err(|e| xml::error_at(node, &e.to_string()))?;
        Ok(value)
    }

    pub(super) fn write(&self, text: &mut String) {
        writeln!(
            text,
            "  <declare name=\"{}\"{}>",
            xml::escape_attribute(&self.name),
            if self.tag { "" } else { " tag=\"false\"" }
        )
        .unwrap();
        for declaration in &self.declarations {
            match declaration {
                ExternalDeclaration::Implements(name) => writeln!(
                    text,
                    "    <implements interface=\"{}\"/>",
                    xml::escape_attribute(name)
                )
                .unwrap(),
                ExternalDeclaration::Input(port) => write_port(text, "input", port, false),
                ExternalDeclaration::OptionalInput(port) => {
                    writeln!(
                        text,
                        "    <input name=\"{}\" type=\"{}\"{} optional=\"true\"/>",
                        port.name,
                        xml::escape_attribute(port.ty.kind.spelling()),
                        if port.ty.fallible {
                            " fallible=\"true\""
                        } else {
                            ""
                        }
                    )
                    .unwrap();
                }
                ExternalDeclaration::Children(c) => write_children(c, text),
                ExternalDeclaration::Impl(i) => i.write(text, 2),
                ExternalDeclaration::Output(output) => {
                    write_port(text, "output", &output.port, output.inline)
                }
            }
        }
        text.push_str("  </declare>\n");
    }
}
fn write_port(text: &mut String, tag: &str, port: &Port, inline: bool) {
    writeln!(
        text,
        "    <{tag} name=\"{}\" type=\"{}\"{}{}/>",
        xml::escape_attribute(&port.name),
        xml::escape_attribute(port.ty.kind.spelling()),
        if port.ty.fallible {
            " fallible=\"true\""
        } else {
            ""
        },
        if inline { " inline=\"true\"" } else { "" }
    )
    .unwrap();
}
fn parse_children(node: roxmltree::Node<'_, '_>) -> Result<Children, Error> {
    xml::check(node, "children", &["type", "min", "max"])?;
    xml::arity(node, 0)?;
    let number = |key| {
        node.attribute(key)
            .map(|s| {
                s.parse::<usize>()
                    .map_err(|_| xml::error_at(node, "invalid child count"))
            })
            .transpose()
    };
    let value = Children {
        kind: Type::parse(xml::attribute(node, "type")?)?,
        minimum: number("min")?.unwrap_or(0),
        maximum: number("max")?,
    };
    value.validate()?;
    Ok(value)
}
fn write_children(value: &Children, out: &mut String) {
    write!(
        out,
        "    <children type=\"{}\" min=\"{}\"",
        value.kind.spelling(),
        value.minimum
    )
    .unwrap();
    if let Some(max) = value.maximum {
        write!(out, " max=\"{max}\"").unwrap();
    }
    out.push_str("/>\n");
}
