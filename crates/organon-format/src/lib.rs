//! Organon's XML storage format and serialization.
//!
//! XML-decoded attribute values retain their authored strings. Template syntax
//! is validated without expanding references. Format types describe accepted XML,
//! separately from the application's editing data and the compiler's internal IR.
//! Canonical XML preserves data, but not source formatting or comments.

mod bindings;
mod element;
mod interface;
mod template;
mod text;
mod xml;

pub use element::{ElementDefinition, Get, Instance, Let, Point, ValueExpr};
pub use interface::{
    Definition, DefinitionFile, InterfaceDefinition, InterfaceOutput, Port, PortType,
};
pub use text::{TextContent, TextStyleOverride};

pub use template::{Segment, Template};

use std::{error::Error as StdError, fmt};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Input {
    pub name: String,
    pub kind: InputType,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputType {
    Element(String),
    Scalar,
    Color,
    ColorField,
    Canvas,
    String,
    Fonts,
    Path,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextStyle {
    pub fonts: String,
    pub font_size: String,
    pub foreground: String,
}

/// The supported XML drawing vocabulary; fields retain attribute text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CanvasExpr {
    Get(Get),
    Draw {
        source: String,
        coverage: CoverageExpr,
    },
    Text {
        content: Vec<TextContent>,
        style: TextStyle,
        wrap_width: Option<String>,
    },
    Translate {
        x: String,
        y: String,
        content: Box<CanvasExpr>,
    },
    SourceOver {
        space: String,
        backdrop: Box<CanvasExpr>,
        source: Box<CanvasExpr>,
    },
    Reference(String),
}

/// An expression producing Theoria Coverage, independently of the color source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoverageExpr {
    All,
    Fill(PathExpr),
}

/// Path construction. Rect is a convenience for a four-point polygon.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PathExpr {
    Get(Get),
    Reference(String),
    Polygon(Vec<Point>),
    Rect {
        x: String,
        y: String,
        width: String,
        height: String,
    },
}

impl CoverageExpr {
    fn validate(&self, depth: usize) -> Result<(), Error> {
        match self {
            Self::All => Ok(()),
            Self::Fill(path) => path.validate_at(depth + 1),
        }
    }
}

impl PathExpr {
    fn validate_at(&self, depth: usize) -> Result<(), Error> {
        if depth > 128 {
            return Err(Error("expression nesting exceeds 128 levels".into()));
        }
        match self {
            Self::Get(value) => value.validate(depth),
            Self::Reference(value) => binding_reference(value),
            Self::Polygon(points) => {
                if points.len() < 3 {
                    return Err(Error("Polygon requires at least three points".into()));
                }
                for point in points {
                    Template::parse(&point.x)?;
                    Template::parse(&point.y)?;
                }
                Ok(())
            }
            Self::Rect {
                x,
                y,
                width,
                height,
            } => {
                for value in [x, y, width, height] {
                    Template::parse(value)?;
                }
                Ok(())
            }
        }
    }
}

/// An XML syntax, structure, or serialization error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error(pub(crate) String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl StdError for Error {}

impl CanvasExpr {
    fn validate(&self, depth: usize) -> Result<(), Error> {
        if depth > 128 {
            return Err(Error("drawing nesting exceeds 128 levels".into()));
        }
        match self {
            Self::Get(value) => value.validate(depth)?,
            Self::Draw { source, coverage } => {
                Template::parse(source)?;
                coverage.validate(depth + 1)?;
            }
            Self::Text {
                content,
                style,
                wrap_width,
            } => {
                for child in content {
                    child.validate(depth + 1)?;
                }
                for value in [&style.font_size, &style.foreground] {
                    Template::parse(value)?;
                }
                resource_reference(&style.fonts, "fonts")?;
                if let Some(width) = wrap_width {
                    Template::parse(width)?;
                }
            }
            Self::Translate { x, y, content } => {
                Template::parse(x)?;
                Template::parse(y)?;
                content.validate(depth + 1)?;
            }
            Self::SourceOver {
                space,
                backdrop,
                source,
            } => {
                if !matches!(
                    space.as_str(),
                    "Srgb" | "LinearSrgb" | "DisplayP3" | "LinearDisplayP3"
                ) {
                    return Err(Error(
                        "space requires a fixed supported composition space".into(),
                    ));
                }
                backdrop.validate(depth + 1)?;
                source.validate(depth + 1)?;
            }
            Self::Reference(value) => binding_reference(value)?,
        }
        Ok(())
    }
}

fn resource_reference(value: &str, attribute: &str) -> Result<(), Error> {
    if Template::parse(value)?.reference().is_some() {
        Ok(())
    } else {
        Err(Error(format!(
            "{attribute} requires a single {{name}} reference"
        )))
    }
}

fn text(value: &str) -> Result<(), Error> {
    if value.chars().all(xml_character) {
        Ok(())
    } else {
        Err(Error(
            "attribute contains a character forbidden by XML 1.0".into(),
        ))
    }
}

fn xml_character(c: char) -> bool {
    matches!(c,
        '\t' | '\n' | '\r'
        | '\u{20}'..='\u{d7ff}'
        | '\u{e000}'..='\u{fffd}'
        | '\u{10000}'..='\u{10ffff}'
    )
}

impl InputType {
    pub fn parse(value: &str) -> Result<Self, Error> {
        Ok(match value {
            "f32" => Self::Scalar,
            "String" => Self::String,
            "Color" => Self::Color,
            "ColorField" => Self::ColorField,
            "Canvas" => Self::Canvas,
            "Path" => Self::Path,
            "Fonts" => Self::Fonts,
            name if template::identifier(name)
                && name.starts_with(|c: char| c.is_ascii_uppercase()) =>
            {
                Self::Element(name.into())
            }
            _ => return Err(Error(format!("unknown input type {value:?}"))),
        })
    }
    pub fn spelling(&self) -> &str {
        match self {
            Self::Scalar => "f32",
            Self::String => "String",
            Self::Color => "Color",
            Self::ColorField => "ColorField",
            Self::Canvas => "Canvas",
            Self::Path => "Path",
            Self::Fonts => "Fonts",
            Self::Element(name) => name,
        }
    }
    pub(crate) fn validate(&self) -> Result<(), Error> {
        if Self::parse(self.spelling())? != *self {
            return Err(Error("invalid element type name".into()));
        }
        Ok(())
    }
}

fn binding_reference(value: &str) -> Result<(), Error> {
    if Template::parse(value)?
        .reference()
        .is_some_and(template::identifier)
    {
        Ok(())
    } else {
        Err(Error(
            "ref requires a single binding name; use get to select an output".into(),
        ))
    }
}

/// Built-in type and expression names cannot also denote a custom definition.
pub(crate) fn primitive_name(name: &str) -> bool {
    matches!(
        name,
        "f32"
            | "String"
            | "Color"
            | "ColorField"
            | "Canvas"
            | "Path"
            | "Fonts"
            | "Rect"
            | "Polygon"
            | "Point"
            | "Draw"
            | "Text"
            | "Translate"
            | "SourceOver"
            | "Fill"
            | "All"
    )
}
