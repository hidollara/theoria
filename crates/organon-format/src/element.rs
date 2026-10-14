//! Named expressions declared directly under Element.
use crate::xml::{
    arity, at, attribute, check, escape_attribute as escape, parse_node, parse_path,
    parse_reference, write_node, write_path, write_reference,
};
use crate::{CanvasExpr, Error, Input, InputType, PathExpr, template};
use std::{collections::HashSet, fmt::Write};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ElementDefinition {
    pub name: String,
    pub inputs: Vec<Input>,
    pub definitions: Vec<Let>,
}

/// One named value declared directly under Element.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Let {
    pub name: String,
    pub export: bool,
    /// Do not introduce an independent reactive cache for this binding.
    pub inline: bool,
    pub value: ValueExpr,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValueExpr {
    Reference(String),
    Path(PathExpr),
    Canvas(CanvasExpr),
    Instance(Instance),
    Get(Get),
}

/// Select one named output from an element expression.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Get {
    pub name: String,
    pub target: Box<ValueExpr>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instance {
    pub element: String,
    pub inputs: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Point {
    pub x: String,
    pub y: String,
}

impl Let {
    fn validate(&self) -> Result<(), Error> {
        if !template::identifier(&self.name) {
            return Err(Error(format!("invalid binding {:?}", self.name)));
        }
        self.value.validate(0)
    }
}

impl Get {
    pub(crate) fn validate(&self, depth: usize) -> Result<(), Error> {
        if !template::identifier(&self.name) {
            return Err(Error("get name requires a fixed output identifier".into()));
        }
        self.target.validate(depth + 1)
    }
}

impl ValueExpr {
    pub(crate) fn validate(&self, depth: usize) -> Result<(), Error> {
        if depth > 128 {
            return Err(Error("expression nesting exceeds 128 levels".into()));
        }
        match self {
            ValueExpr::Get(value) => value.validate(depth),
            ValueExpr::Reference(value) => crate::binding_reference(value),
            ValueExpr::Path(value) => value.validate_at(depth),
            ValueExpr::Canvas(value) => value.validate(depth),
            ValueExpr::Instance(value) => {
                if !template::identifier(&value.element)
                    || crate::primitive_name(&value.element)
                    || !value.element.starts_with(|c: char| c.is_ascii_uppercase())
                {
                    return Err(Error("an instance must have a valid type name".into()));
                }
                let mut names = HashSet::new();
                for (name, value) in &value.inputs {
                    if !template::identifier(name) || !names.insert(name) {
                        return Err(Error(format!(
                            "invalid or duplicate instance input {name:?}"
                        )));
                    }
                    crate::Template::parse(value)?;
                }
                Ok(())
            }
        }
    }
}

impl ElementDefinition {
    pub fn validate(&self) -> Result<(), Error> {
        if !template::identifier(&self.name) || crate::primitive_name(&self.name) {
            return Err(Error(format!(
                "invalid or reserved element name {:?}",
                self.name
            )));
        }
        let mut names = HashSet::new();
        for input in &self.inputs {
            input.kind.validate()?;
            if !template::identifier(&input.name) || !names.insert(input.name.clone()) {
                return Err(Error(format!(
                    "invalid or duplicate binding {:?}",
                    input.name
                )));
            }
        }
        for definition in &self.definitions {
            definition.validate()?;
            if !names.insert(definition.name.clone()) {
                return Err(Error(format!("duplicate binding {:?}", definition.name)));
            }
        }
        if !self.definitions.iter().any(|d| d.export) {
            return Err(Error("element requires at least one exported let".into()));
        }
        Ok(())
    }

    pub fn from_xml(source: &str) -> Result<Self, Error> {
        crate::xml::check_source_depth(source)?;
        let document = roxmltree::Document::parse(source).map_err(|e| Error(e.to_string()))?;
        let root = document.root_element();
        check(root, "element", &["name", "version"])?;
        if attribute(root, "version")? != "1" {
            return Err(at(root, "unsupported element version (expected 1)"));
        }
        Self::parse_body(root)
    }

    pub(crate) fn from_node(root: roxmltree::Node<'_, '_>) -> Result<Self, Error> {
        check(root, "element", &["name"])?;
        Self::parse_body(root)
    }

    fn parse_body(root: roxmltree::Node<'_, '_>) -> Result<Self, Error> {
        let mut result = Self {
            name: attribute(root, "name")?.into(),
            inputs: vec![],
            definitions: vec![],
        };
        for child in root.children().filter(roxmltree::Node::is_element) {
            match child.tag_name().name() {
                "input" => {
                    check(child, "input", &["name", "type"])?;
                    arity(child, 0)?;
                    let kind = InputType::parse(attribute(child, "type")?)
                        .map_err(|e| at(child, &e.to_string()))?;
                    result.inputs.push(Input {
                        name: attribute(child, "name")?.into(),
                        kind,
                    });
                }
                "let" => result.definitions.push(parse_let(child)?),
                _ => return Err(at(child, "expected input or let")),
            }
        }
        result.validate().map_err(|e| at(root, &e.to_string()))?;
        Ok(result)
    }

    pub fn to_xml(&self) -> Result<String, Error> {
        self.validate()?;
        let mut xml = format!("<element name=\"{}\" version=\"1\">\n", escape(&self.name));
        for input in &self.inputs {
            let kind = input.kind.spelling();
            writeln!(
                xml,
                "  <input name=\"{}\" type=\"{kind}\"/>",
                escape(&input.name)
            )
            .unwrap();
        }
        for definition in &self.definitions {
            write_let(definition, 1, &mut xml);
        }
        xml.push_str("</element>\n");
        Ok(xml)
    }
}

fn parse_let(node: roxmltree::Node<'_, '_>) -> Result<Let, Error> {
    check(node, "let", &["name", "export", "inline"])?;
    let export = match node.attribute("export") {
        None | Some("false") => false,
        Some("true") => true,
        _ => return Err(at(node, "export requires a fixed true or false")),
    };
    let child = arity(node, 1)?[0];
    let value = parse_value(child, 0)?;
    Ok(Let {
        name: attribute(node, "name")?.into(),
        export,
        inline: match node.attribute("inline") {
            None | Some("false") => false,
            Some("true") => true,
            _ => return Err(at(node, "inline requires a fixed true or false")),
        },
        value,
    })
}

fn parse_value(child: roxmltree::Node<'_, '_>, depth: usize) -> Result<ValueExpr, Error> {
    if depth > 128 {
        return Err(at(child, "expression nesting exceeds 128 levels"));
    }
    Ok(match child.tag_name().name() {
        "let" => return Err(at(child, "let is only allowed directly under element")),
        "get" => ValueExpr::Get(parse_get(child, depth)?),
        "ref" => ValueExpr::Reference(parse_reference(child)?),
        "Rect" | "Polygon" => ValueExpr::Path(parse_path(child, depth)?),
        "Draw" | "Text" | "Translate" | "SourceOver" => {
            ValueExpr::Canvas(parse_node(child, depth)?)
        }
        name if name.starts_with(|c: char| c.is_ascii_uppercase())
            && !crate::primitive_name(name) =>
        {
            let attributes: Vec<_> = child.attributes().map(|a| a.name()).collect();
            check(child, name, &attributes)?;
            arity(child, 0)?;
            ValueExpr::Instance(Instance {
                element: name.into(),
                inputs: child
                    .attributes()
                    .map(|a| (a.name().into(), a.value().into()))
                    .collect(),
            })
        }
        _ => return Err(at(child, "expected a primitive or named element")),
    })
}

pub(crate) fn parse_get(node: roxmltree::Node<'_, '_>, depth: usize) -> Result<Get, Error> {
    check(node, "get", &["name"])?;
    Ok(Get {
        name: attribute(node, "name")?.into(),
        target: Box::new(parse_value(arity(node, 1)?[0], depth + 1)?),
    })
}

fn write_let(value: &Let, depth: usize, xml: &mut String) {
    let pad = "  ".repeat(depth);
    write!(xml, "{pad}<let name=\"{}\"", escape(&value.name)).unwrap();
    if value.export {
        xml.push_str(" export=\"true\"");
    }
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
        ValueExpr::Get(value) => write_get(value, depth, xml),
        ValueExpr::Reference(value) => write_reference(value, depth, xml),
        ValueExpr::Path(value) => write_path(value, depth, xml),
        ValueExpr::Canvas(value) => write_node(value, depth, xml),
        ValueExpr::Instance(value) => {
            write!(xml, "{pad}<{}", value.element).unwrap();
            for (name, value) in &value.inputs {
                write!(xml, " {name}=\"{}\"", escape(value)).unwrap();
            }
            xml.push_str("/>\n");
        }
    }
}

pub(crate) fn write_get(value: &Get, depth: usize, xml: &mut String) {
    let pad = "  ".repeat(depth);
    writeln!(xml, "{pad}<get name=\"{}\">", escape(&value.name)).unwrap();
    write_value(&value.target, depth + 1, xml);
    writeln!(xml, "{pad}</get>").unwrap();
}
