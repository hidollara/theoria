//! Application-owned editing data, separate from format types and compiler IR.
pub use format::{CoverageExpr, PathExpr, TextContent, TextStyleOverride};
use organon_format as format;

/// Authored text is retained during editing, including unexpanded references.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextStyle {
    pub fonts: String,
    pub font_size: String,
    pub foreground: String,
}

/// An editable expression that produces a Theoria Canvas when evaluated.
/// This describes drawing only; the enclosing Element owns inputs and outputs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CanvasExpr {
    Get(format::Get),
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

impl From<format::CanvasExpr> for CanvasExpr {
    fn from(node: format::CanvasExpr) -> Self {
        match node {
            format::CanvasExpr::Get(value) => Self::Get(value),
            format::CanvasExpr::Draw { source, coverage } => Self::Draw { source, coverage },
            format::CanvasExpr::Text {
                content,
                style,
                wrap_width,
            } => Self::Text {
                content,
                wrap_width,
                style: TextStyle {
                    fonts: style.fonts,
                    font_size: style.font_size,
                    foreground: style.foreground,
                },
            },
            format::CanvasExpr::Translate { x, y, content } => Self::Translate {
                x,
                y,
                content: Box::new((*content).into()),
            },
            format::CanvasExpr::SourceOver {
                space,
                backdrop,
                source,
            } => Self::SourceOver {
                space,
                backdrop: Box::new((*backdrop).into()),
                source: Box::new((*source).into()),
            },
            format::CanvasExpr::Reference(name) => Self::Reference(name),
        }
    }
}

impl From<CanvasExpr> for format::CanvasExpr {
    fn from(expr: CanvasExpr) -> Self {
        match expr {
            CanvasExpr::Get(value) => Self::Get(value),
            CanvasExpr::Draw { source, coverage } => Self::Draw { source, coverage },
            CanvasExpr::Text {
                content,
                style,
                wrap_width,
            } => Self::Text {
                content,
                wrap_width,
                style: format::TextStyle {
                    fonts: style.fonts,
                    font_size: style.font_size,
                    foreground: style.foreground,
                },
            },
            CanvasExpr::Translate { x, y, content } => Self::Translate {
                x,
                y,
                content: Box::new((*content).into()),
            },
            CanvasExpr::SourceOver {
                space,
                backdrop,
                source,
            } => Self::SourceOver {
                space,
                backdrop: Box::new((*backdrop).into()),
                source: Box::new((*source).into()),
            },
            CanvasExpr::Reference(name) => Self::Reference(name),
        }
    }
}

/// An editable definition. Only exported bindings form its public interface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Element {
    pub name: String,
    pub inputs: Vec<format::Input>,
    pub definitions: Vec<Let>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Let {
    pub name: String,
    pub export: bool,
    pub inline: bool,
    pub value: ValueExpr,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValueExpr {
    Get(format::Get),
    Reference(String),
    Path(PathExpr),
    Canvas(CanvasExpr),
    Instance(format::Instance),
}

impl Element {
    pub fn from_xml(source: &str) -> Result<Self, format::Error> {
        format::ElementDefinition::from_xml(source).map(Into::into)
    }
    pub fn to_xml(&self) -> Result<String, format::Error> {
        format::ElementDefinition::from(self.clone()).to_xml()
    }
}
impl From<format::ElementDefinition> for Element {
    fn from(value: format::ElementDefinition) -> Self {
        Self {
            name: value.name,
            inputs: value.inputs,
            definitions: value.definitions.into_iter().map(Into::into).collect(),
        }
    }
}
impl From<Element> for format::ElementDefinition {
    fn from(value: Element) -> Self {
        Self {
            name: value.name,
            inputs: value.inputs,
            definitions: value.definitions.into_iter().map(Into::into).collect(),
        }
    }
}
impl From<format::Let> for Let {
    fn from(value: format::Let) -> Self {
        Self {
            name: value.name,
            export: value.export,
            inline: value.inline,
            value: match value.value {
                format::ValueExpr::Get(value) => ValueExpr::Get(value),
                format::ValueExpr::Reference(value) => ValueExpr::Reference(value),
                format::ValueExpr::Path(value) => ValueExpr::Path(value),
                format::ValueExpr::Canvas(value) => ValueExpr::Canvas(value.into()),
                format::ValueExpr::Instance(value) => ValueExpr::Instance(value),
            },
        }
    }
}
impl From<Let> for format::Let {
    fn from(value: Let) -> Self {
        Self {
            name: value.name,
            export: value.export,
            inline: value.inline,
            value: match value.value {
                ValueExpr::Get(value) => format::ValueExpr::Get(value),
                ValueExpr::Reference(value) => format::ValueExpr::Reference(value),
                ValueExpr::Path(value) => format::ValueExpr::Path(value),
                ValueExpr::Canvas(value) => format::ValueExpr::Canvas(value.into()),
                ValueExpr::Instance(value) => format::ValueExpr::Instance(value),
            },
        }
    }
}
