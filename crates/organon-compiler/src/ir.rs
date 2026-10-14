use organon_format::{Segment, Template};
use std::collections::HashMap;

use super::Error;

/// Private typed representation used only for validation and Rust generation.
#[derive(Clone, Debug, PartialEq)]
pub struct DrawingIr {
    pub inputs: Vec<Input>,
    pub root: CanvasExpr,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Input {
    pub name: String,
    pub kind: InputType,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputType {
    Element,
    Scalar,
    Color,
    ColorField,
    Canvas,
    String,
    Fonts,
    Path,
}

/// A folded literal, a direct input, or text expanded before interpretation.
#[derive(Clone, Debug, PartialEq)]
pub enum Value<T> {
    Literal(T),
    Input(String),
    Template(Template),
}

/// Organon's color-code literal: straight-alpha sRGB bytes. Native Canvas
/// colors from any supported space can instead be supplied through an input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color(pub [u8; 4]);

/// A solid color or a native ColorField/Canvas supplied by the host.
#[derive(Clone, Debug, PartialEq)]
pub enum ColorFieldExpr {
    Solid(Value<Color>),
    Input(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Space {
    Srgb,
    LinearSrgb,
    DisplayP3,
    LinearDisplayP3,
}

/// Resolved syntax for typography used by the code generator.
/// `fonts` names an ordered font-chain input; font loading belongs to the host.
#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    pub fonts: String,
    pub font_size: Value<f32>,
    pub foreground: ColorFieldExpr,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TextContent {
    String {
        content: Value<String>,
        style: TextStyle,
    },
    Span {
        children: Vec<TextContent>,
        style: TextStyle,
    },
}

impl TextContent {
    fn validate(&self, inputs: &HashMap<&str, InputType>) -> Result<(), Error> {
        match self {
            Self::String { content, style } => {
                if let Value::Input(name) = content {
                    binding(name, InputType::String, inputs)?;
                }
                if let Value::Template(template) = content {
                    interpolation(template, inputs)?;
                }
                style.validate(inputs)
            }
            Self::Span { children, style } => {
                style.validate(inputs)?;
                for child in children {
                    child.validate(inputs)?;
                }
                Ok(())
            }
        }
    }
}

impl TextStyle {
    pub(crate) fn validate(&self, inputs: &HashMap<&str, InputType>) -> Result<(), Error> {
        binding(&self.fonts, InputType::Fonts, inputs)?;
        validate_scalar(&self.font_size, true, inputs)?;
        validate_color_field(&self.foreground, inputs)
    }
}

/// Initial drawing vocabulary. Composition order is explicit: backdrop first,
/// then source. Richer Rust-built drawings can be embedded with `ref`.
#[derive(Clone, Debug, PartialEq)]
pub enum CanvasExpr {
    Draw {
        source: ColorFieldExpr,
        coverage: CoverageExpr,
    },
    /// Horizontal text at the origin. Wrapping never introduces clipping.
    Text {
        content: Vec<TextContent>,
        style: TextStyle,
        wrap_width: Option<Value<f32>>,
    },
    Translate {
        x: Value<f32>,
        y: Value<f32>,
        content: Box<CanvasExpr>,
    },
    SourceOver {
        space: Space,
        backdrop: Box<CanvasExpr>,
        source: Box<CanvasExpr>,
    },
    Reference(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum CoverageExpr {
    All,
    Fill(PathExpr),
}

impl CanvasExpr {
    /// Whether this operation can fail, before accounting for referenced Lets.
    pub(crate) fn is_fallible(&self) -> bool {
        match self {
            Self::Text { .. } | Self::Translate { .. } => true,
            Self::Draw { source, coverage } => {
                matches!(source, ColorFieldExpr::Solid(Value::Template(_)))
                    || matches!(coverage, CoverageExpr::Fill(_))
            }
            Self::SourceOver {
                backdrop, source, ..
            } => backdrop.is_fallible() || source.is_fallible(),
            Self::Reference(_) => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PathExpr {
    Reference(String),
    Polygon(Vec<(Value<f32>, Value<f32>)>),
    Rect {
        x: Value<f32>,
        y: Value<f32>,
        width: Value<f32>,
        height: Value<f32>,
    },
}

impl PathExpr {
    pub(crate) fn validate(&self, inputs: &HashMap<&str, InputType>) -> Result<(), Error> {
        let scalar = |value, nonnegative| validate_scalar(value, nonnegative, inputs);
        match self {
            Self::Reference(name) => binding(name, InputType::Path, inputs),
            Self::Polygon(points) => {
                for (x, y) in points {
                    scalar(x, false)?;
                    scalar(y, false)?;
                }
                Ok(())
            }
            Self::Rect {
                x,
                y,
                width,
                height,
            } => {
                scalar(x, false)?;
                scalar(y, false)?;
                scalar(width, true)?;
                scalar(height, true)?;
                for (origin, extent) in [(x, width), (y, height)] {
                    if let (Value::Literal(origin), Value::Literal(extent)) = (origin, extent)
                        && !(origin + extent).is_finite()
                    {
                        return Err(Error("rectangle endpoint overflows f32".into()));
                    }
                }
                Ok(())
            }
        }
    }
}

impl DrawingIr {
    /// Checks input bindings and literal values after lowering from storage.
    pub fn validate(&self) -> Result<(), Error> {
        let mut inputs = HashMap::new();
        for input in &self.inputs {
            if !identifier(&input.name) {
                return Err(Error(format!("invalid input name {:?}", input.name)));
            }
            if inputs.insert(input.name.as_str(), input.kind).is_some() {
                return Err(Error(format!("duplicate input {:?}", input.name)));
            }
        }
        self.root.validate(&inputs, 0)
    }
}

pub(crate) fn identifier(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !matches!(name, "_" | "self" | "Self" | "super" | "crate")
}

fn binding(name: &str, kind: InputType, inputs: &HashMap<&str, InputType>) -> Result<(), Error> {
    match inputs.get(name) {
        Some(actual) if *actual == kind => Ok(()),
        Some(actual) => Err(Error(format!(
            "input {name:?} is {actual:?}, expected {kind:?}"
        ))),
        None => Err(Error(format!("unknown input {name:?}"))),
    }
}

fn interpolation(template: &Template, inputs: &HashMap<&str, InputType>) -> Result<(), Error> {
    for segment in template.segments() {
        if let Segment::Reference(name) = segment {
            match inputs.get(name.as_str()) {
                Some(InputType::Scalar | InputType::String) => (),
                Some(kind) => {
                    return Err(Error(format!(
                        "input {name:?} is {kind:?}, not a textual value"
                    )));
                }
                None => return Err(Error(format!("unknown input {name:?}"))),
            }
        }
    }
    Ok(())
}

fn validate_color(value: &Value<Color>, inputs: &HashMap<&str, InputType>) -> Result<(), Error> {
    match value {
        Value::Literal(_) => Ok(()),
        Value::Input(name) => binding(name, InputType::Color, inputs),
        Value::Template(template) => interpolation(template, inputs),
    }
}

fn validate_color_field(
    value: &ColorFieldExpr,
    inputs: &HashMap<&str, InputType>,
) -> Result<(), Error> {
    match value {
        ColorFieldExpr::Solid(color) => validate_color(color, inputs),
        ColorFieldExpr::Input(name) => match inputs.get(name.as_str()) {
            Some(InputType::ColorField | InputType::Canvas) => Ok(()),
            _ => Err(Error(format!(
                "input {name:?} must supply a ColorField or Canvas"
            ))),
        },
    }
}

impl CanvasExpr {
    fn validate(&self, inputs: &HashMap<&str, InputType>, depth: usize) -> Result<(), Error> {
        if depth > 128 {
            return Err(Error("drawing nesting exceeds 128 levels".into()));
        }
        let scalar =
            |value: &Value<f32>, nonnegative: bool| validate_scalar(value, nonnegative, inputs);
        match self {
            Self::Draw { source, coverage } => {
                validate_color_field(source, inputs)?;
                match coverage {
                    CoverageExpr::All => Ok(()),
                    CoverageExpr::Fill(path) => path.validate(inputs),
                }
            }
            Self::Text {
                content,
                style,
                wrap_width,
            } => {
                for span in content {
                    span.validate(inputs)?;
                }
                style.validate(inputs)?;
                if let Some(width) = wrap_width {
                    scalar(width, true)?;
                }
                Ok(())
            }
            Self::Translate { x, y, content } => {
                scalar(x, false)?;
                scalar(y, false)?;
                content.validate(inputs, depth + 1)
            }
            Self::SourceOver {
                backdrop, source, ..
            } => {
                backdrop.validate(inputs, depth + 1)?;
                source.validate(inputs, depth + 1)
            }
            Self::Reference(name) => binding(name, InputType::Canvas, inputs),
        }
    }
}

pub(crate) fn validate_scalar(
    value: &Value<f32>,
    nonnegative: bool,
    inputs: &HashMap<&str, InputType>,
) -> Result<(), Error> {
    match value {
        Value::Literal(value) if !value.is_finite() || (nonnegative && *value < 0.0) => Err(Error(
            "coordinates must be finite; dimensions must be nonnegative".into(),
        )),
        Value::Literal(_) => Ok(()),
        Value::Input(name) => binding(name, InputType::Scalar, inputs),
        Value::Template(template) => interpolation(template, inputs),
    }
}
