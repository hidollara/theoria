//! Inline text syntax. Literal text enters only through `s.t` attributes.
use std::fmt::Write;

use roxmltree::Node;

use crate::{Error, Template, TextStyle, resource_reference, xml};

/// Only authored overrides are stored; consumers resolve them against the parent.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextStyleOverride {
    pub fonts: Option<String>,
    pub font_size: Option<String>,
    pub foreground: Option<String>,
}

/// A string leaf, a styled range, or an explicit newline. Boundaries do not
/// force shaping boundaries: compatible adjacent fragments may shape together.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextContent {
    String {
        template: String,
        style: TextStyleOverride,
    },
    Span {
        style: TextStyleOverride,
        children: Vec<TextContent>,
    },
    Break,
}

impl TextStyleOverride {
    pub fn resolve(&self, parent: &TextStyle) -> TextStyle {
        TextStyle {
            fonts: self.fonts.clone().unwrap_or_else(|| parent.fonts.clone()),
            font_size: self
                .font_size
                .clone()
                .unwrap_or_else(|| parent.font_size.clone()),
            foreground: self
                .foreground
                .clone()
                .unwrap_or_else(|| parent.foreground.clone()),
        }
    }

    fn validate(&self) -> Result<(), Error> {
        if let Some(fonts) = &self.fonts {
            resource_reference(fonts, "fonts")?;
        }
        for value in [&self.font_size, &self.foreground].into_iter().flatten() {
            Template::parse(value)?;
        }
        Ok(())
    }

    fn attributes<'a>(&'a self, attributes: &mut Vec<&'a str>) {
        attributes.extend(
            [&self.fonts, &self.font_size, &self.foreground]
                .into_iter()
                .filter_map(|value| value.as_deref()),
        );
    }
}

impl TextContent {
    pub fn string(template: impl Into<String>) -> Self {
        Self::String {
            template: template.into(),
            style: TextStyleOverride::default(),
        }
    }

    pub(crate) fn validate(&self, depth: usize) -> Result<(), Error> {
        if depth > 128 {
            return Err(Error("text nesting exceeds 128 levels".into()));
        }
        match self {
            Self::String { template, style } => {
                Template::parse(template)?;
                style.validate()?;
            }
            Self::Span { style, children } => {
                style.validate()?;
                for child in children {
                    child.validate(depth + 1)?;
                }
            }
            Self::Break => (),
        }
        Ok(())
    }

    pub(crate) fn attributes<'a>(&'a self, attributes: &mut Vec<&'a str>) {
        match self {
            Self::String { template, style } => {
                attributes.push(template);
                style.attributes(attributes);
            }
            Self::Span { style, children } => {
                style.attributes(attributes);
                for child in children {
                    child.attributes(attributes);
                }
            }
            Self::Break => (),
        }
    }
}

pub(crate) fn parse(node: Node<'_, '_>, depth: usize) -> Result<TextContent, Error> {
    if depth > 128 {
        return Err(xml::at(node, "text nesting exceeds 128 levels"));
    }
    let style = || TextStyleOverride {
        fonts: node.attribute("fonts").map(str::to_owned),
        font_size: node.attribute("font_size").map(str::to_owned),
        foreground: node.attribute("foreground").map(str::to_owned),
    };
    Ok(match node.tag_name().name() {
        "s" => {
            xml::check(node, "s", &["t", "fonts", "font_size", "foreground"])?;
            xml::arity(node, 0)?;
            TextContent::String {
                template: xml::attribute(node, "t")?.into(),
                style: style(),
            }
        }
        "span" => {
            xml::check(node, "span", &["fonts", "font_size", "foreground"])?;
            TextContent::Span {
                style: style(),
                children: node
                    .children()
                    .filter(Node::is_element)
                    .map(|child| parse(child, depth + 1))
                    .collect::<Result<_, _>>()?,
            }
        }
        "br" => {
            xml::check(node, "br", &[])?;
            xml::arity(node, 0)?;
            TextContent::Break
        }
        _ => return Err(xml::at(node, "expected text content: s, span, or br")),
    })
}

fn write_style(style: &TextStyleOverride, output: &mut String) {
    for (name, value) in [
        ("fonts", &style.fonts),
        ("font_size", &style.font_size),
        ("foreground", &style.foreground),
    ] {
        if let Some(value) = value {
            write!(output, " {name}=\"{}\"", xml::escape_attribute(value)).unwrap();
        }
    }
}

pub(crate) fn write(node: &TextContent, depth: usize, output: &mut String) {
    let pad = "  ".repeat(depth);
    match node {
        TextContent::String { template, style } => {
            write!(output, "{pad}<s t=\"{}\"", xml::escape_attribute(template)).unwrap();
            write_style(style, output);
            writeln!(output, "/>").unwrap();
        }
        TextContent::Span { style, children } => {
            write!(output, "{pad}<span").unwrap();
            write_style(style, output);
            writeln!(output, ">").unwrap();
            for child in children {
                write(child, depth + 1, output);
            }
            writeln!(output, "{pad}</span>").unwrap();
        }
        TextContent::Break => writeln!(output, "{pad}<br/>").unwrap(),
    }
}
