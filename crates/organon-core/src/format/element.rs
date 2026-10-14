//! Named expressions declared directly under Element.

use crate::format::xml::{
    arity, attribute, boolean, check, error_at, escape_attribute as escape, parse_reference,
    write_reference,
};
use crate::format::{Error, Input, Provide, Type, names};
use std::{collections::BTreeMap, fmt::Write};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ElementDefinition {
    pub name: String,
    pub declarations: Vec<Declaration>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Declaration {
    Impl(super::Implementation),
    Input(Input),
    Let(Let),
    Bind(Bind),
    Provide(Provide),
}

/// Destructure one value into local names; only valid directly under Element.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bind {
    /// (public output name, local binding name).
    pub outputs: BTreeMap<String, String>,
    pub value: ValueExpr,
}

/// One named value declared directly under Element.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Let {
    pub name: String,
    /// Do not introduce an independent reactive cache for this binding.
    pub inline: bool,
    pub value: ValueExpr,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValueExpr {
    Reference(String),
    Instance(Instance),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instance {
    pub element: String,
    /// XML-decoded text; parsed after the receiving Input types are resolved.
    pub inputs: BTreeMap<String, String>,
    pub children: Vec<ValueExpr>,
}

impl Let {
    fn validate(&self) -> Result<(), Error> {
        if !names::identifier(&self.name) {
            return Err(Error(format!("invalid binding {:?}", self.name)));
        }
        self.value.validate(0)
    }
}

impl ValueExpr {
    fn validate(&self, depth: usize) -> Result<(), Error> {
        if depth > 128 {
            return Err(Error("expression nesting exceeds 128 levels".into()));
        }
        match self {
            ValueExpr::Reference(value) => value.parse::<names::BindingName>().map(|_| ()),
            ValueExpr::Instance(value) => {
                if !names::identifier(&value.element)
                    || crate::format::reserved_name(&value.element)
                {
                    return Err(Error("an instance must have a valid type name".into()));
                }
                for (name, value) in &value.inputs {
                    super::xml::text(value)?;
                    if name == "xmlns" || !names::identifier(name) {
                        return Err(Error(format!("invalid instance input {name:?}")));
                    }
                }
                for child in &value.children {
                    child.validate(depth + 1)?;
                }
                Ok(())
            }
        }
    }
}

impl ElementDefinition {
    pub fn implementations(&self) -> impl Iterator<Item = &super::Implementation> {
        self.declarations.iter().filter_map(|d| match d {
            Declaration::Impl(v) => Some(v),
            _ => None,
        })
    }

    pub fn inputs(&self) -> impl Iterator<Item = &Input> {
        self.declarations.iter().filter_map(|d| match d {
            Declaration::Input(v) => Some(v),
            _ => None,
        })
    }
    pub fn lets(&self) -> impl Iterator<Item = &Let> {
        self.declarations.iter().filter_map(|d| match d {
            Declaration::Let(v) => Some(v),
            _ => None,
        })
    }
    pub fn inputs_mut(&mut self) -> impl Iterator<Item = &mut Input> {
        self.declarations.iter_mut().filter_map(|d| match d {
            Declaration::Input(v) => Some(v),
            _ => None,
        })
    }
    pub fn lets_mut(&mut self) -> impl Iterator<Item = &mut Let> {
        self.declarations.iter_mut().filter_map(|d| match d {
            Declaration::Let(v) => Some(v),
            _ => None,
        })
    }
    pub fn binds(&self) -> impl Iterator<Item = &Bind> {
        self.declarations.iter().filter_map(|d| match d {
            Declaration::Bind(v) => Some(v),
            _ => None,
        })
    }
    pub fn provides(&self) -> impl Iterator<Item = &Provide> {
        self.declarations.iter().filter_map(|d| match d {
            Declaration::Provide(v) => Some(v),
            _ => None,
        })
    }
    /// Check representability in the Organon format, without resolving names.
    pub fn validate(&self) -> Result<(), Error> {
        if !names::identifier(&self.name) || crate::format::reserved_name(&self.name) {
            return Err(Error(format!(
                "invalid or reserved element name {:?}",
                self.name
            )));
        }
        for declaration in &self.declarations {
            match declaration {
                Declaration::Impl(value) => value.validate()?,
                Declaration::Input(input) => {
                    input.kind.validate()?;
                    if !names::identifier(&input.name) {
                        return Err(Error(format!("invalid input name {:?}", input.name)));
                    }
                }
                Declaration::Let(value) => value.validate()?,
                Declaration::Bind(bind) => {
                    bind.value.validate(0)?;
                    if bind.outputs.is_empty() {
                        return Err(Error("bind requires at least one output binding".into()));
                    }
                    for (field, name) in &bind.outputs {
                        if field == "xmlns" || !names::identifier(field) || !names::identifier(name)
                        {
                            return Err(Error(format!(
                                "invalid output binding {field:?}={name:?}"
                            )));
                        }
                    }
                }
                Declaration::Provide(provide) => provide.validate()?,
            }
        }
        Ok(())
    }

    pub(super) fn from_node(root: roxmltree::Node<'_, '_>) -> Result<Self, Error> {
        check(root, "element", &["name"])?;
        let mut result = Self {
            name: attribute(root, "name")?.into(),
            declarations: vec![],
        };
        for child in root.children().filter(roxmltree::Node::is_element) {
            match child.tag_name().name() {
                "impl" => result
                    .declarations
                    .push(Declaration::Impl(super::Implementation::parse(child)?)),
                "input" => {
                    check(child, "input", &["name", "type"])?;
                    arity(child, 0)?;
                    let kind = Type::parse(attribute(child, "type")?)
                        .map_err(|e| error_at(child, &e.to_string()))?;
                    result.declarations.push(Declaration::Input(Input {
                        name: attribute(child, "name")?.into(),
                        kind,
                    }));
                }
                "let" => result
                    .declarations
                    .push(Declaration::Let(parse_let(child)?)),
                "bind" => {
                    let attrs: Vec<_> = child.attributes().map(|a| a.name()).collect();
                    check(child, "bind", &attrs)?;
                    result.declarations.push(Declaration::Bind(Bind {
                        outputs: child
                            .attributes()
                            .map(|a| (a.name().into(), a.value().into()))
                            .collect(),
                        value: parse_value(arity(child, 1)?[0], 0)?,
                    }));
                }
                "provide" => result
                    .declarations
                    .push(Declaration::Provide(Provide::parse(child)?)),
                _ => {
                    return Err(error_at(
                        child,
                        "expected input, let, bind, provide, or impl",
                    ));
                }
            }
        }
        result
            .validate()
            .map_err(|e| error_at(root, &e.to_string()))?;
        Ok(result)
    }

    pub(super) fn write(&self, xml: &mut String) {
        writeln!(xml, "  <element name=\"{}\">", escape(&self.name)).unwrap();
        for declaration in &self.declarations {
            match declaration {
                Declaration::Impl(value) => value.write(xml, 2),
                Declaration::Input(input) => {
                    writeln!(
                        xml,
                        "    <input name=\"{}\" type=\"{}\"/>",
                        escape(&input.name),
                        input.kind.spelling()
                    )
                    .unwrap();
                }
                Declaration::Let(value) => write_let(value, 2, xml),
                Declaration::Bind(bind) => {
                    xml.push_str("    <bind");
                    for (field, name) in &bind.outputs {
                        write!(xml, " {field}=\"{}\"", escape(name)).unwrap();
                    }
                    xml.push_str(">\n");
                    write_value(&bind.value, 3, xml);
                    xml.push_str("    </bind>\n");
                }
                Declaration::Provide(provide) => provide.write(xml, 2),
            }
        }
        xml.push_str("  </element>\n");
    }
}

fn parse_let(node: roxmltree::Node<'_, '_>) -> Result<Let, Error> {
    check(node, "let", &["name", "inline"])?;
    let child = arity(node, 1)?[0];
    let value = parse_value(child, 0)?;
    Ok(Let {
        name: attribute(node, "name")?.into(),
        inline: boolean(node, "inline", false)?,
        value,
    })
}

fn parse_value(child: roxmltree::Node<'_, '_>, depth: usize) -> Result<ValueExpr, Error> {
    if depth > 128 {
        return Err(error_at(child, "expression nesting exceeds 128 levels"));
    }
    let name = child.tag_name().name();
    if name == "ref" {
        return Ok(ValueExpr::Reference(parse_reference(child)?));
    }
    if crate::format::reserved_name(name) || !names::identifier(name) {
        return Err(error_at(child, "expected an element expression"));
    }
    let attributes: Vec<_> = child.attributes().map(|a| a.name()).collect();
    check(child, name, &attributes)?;
    Ok(ValueExpr::Instance(Instance {
        element: name.into(),
        inputs: child
            .attributes()
            .map(|a| (a.name().into(), a.value().into()))
            .collect(),
        children: child
            .children()
            .filter(roxmltree::Node::is_element)
            .map(|c| parse_value(c, depth + 1))
            .collect::<Result<_, _>>()?,
    }))
}

fn write_let(value: &Let, depth: usize, xml: &mut String) {
    let pad = "  ".repeat(depth);
    write!(xml, "{pad}<let name=\"{}\"", escape(&value.name)).unwrap();
    if value.inline {
        xml.push_str(" inline=\"true\"");
    }
    xml.push_str(">\n");
    write_value(&value.value, depth + 1, xml);
    writeln!(xml, "{pad}</let>").unwrap();
}

fn write_value(value: &ValueExpr, depth: usize, xml: &mut String) {
    let pad = "  ".repeat(depth);
    match value {
        ValueExpr::Reference(value) => write_reference(value, depth, xml),
        ValueExpr::Instance(value) => {
            write!(xml, "{pad}<{}", value.element).unwrap();
            for (name, value) in &value.inputs {
                write!(xml, " {name}=\"{}\"", escape(value)).unwrap();
            }
            if value.children.is_empty() {
                xml.push_str("/>\n");
            } else {
                xml.push_str(">\n");
                for child in &value.children {
                    write_value(child, depth + 1, xml);
                }
                writeln!(xml, "{pad}</{}>", value.element).unwrap();
            }
        }
    }
}

impl ValueExpr {
    pub fn from_xml(source: &str) -> Result<Self, Error> {
        let document = roxmltree::Document::parse(source).map_err(|e| Error(e.to_string()))?;
        let value = parse_value(document.root_element(), 0)?;
        value.validate(0)?;
        Ok(value)
    }
    pub fn to_xml(&self) -> Result<String, Error> {
        self.validate(0)?;
        let mut xml = String::new();
        write_value(self, 0, &mut xml);
        Ok(xml)
    }
}
