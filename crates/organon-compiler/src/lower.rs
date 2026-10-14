//! Converts storage data to a private, checked code-generation representation.
use organon_format as format;

use crate::{
    Error,
    ir::{
        CanvasExpr, Color, ColorFieldExpr, CoverageExpr, DrawingIr, Input, InputType, PathExpr,
        Space, TextStyle, Value,
    },
};

pub(crate) fn lower(
    root: &format::CanvasExpr,
    inputs: &[format::Input],
) -> Result<DrawingIr, Error> {
    Ok(DrawingIr {
        inputs: lower_inputs(inputs),
        root: lower_node(root, inputs)?,
    })
}

pub(crate) fn lower_inputs(inputs: &[format::Input]) -> Vec<Input> {
    inputs
        .iter()
        .map(|input| Input {
            name: input.name.clone(),
            kind: match input.kind {
                format::InputType::Scalar => InputType::Scalar,
                format::InputType::Color => InputType::Color,
                format::InputType::ColorField => InputType::ColorField,
                format::InputType::Canvas => InputType::Canvas,
                format::InputType::String => InputType::String,
                format::InputType::Fonts => InputType::Fonts,
                format::InputType::Path => InputType::Path,
                format::InputType::Element(_) => InputType::Element,
            },
        })
        .collect()
}

fn template(value: &str) -> Result<format::Template, Error> {
    format::Template::parse(value).map_err(|error| Error(error.to_string()))
}

fn typed_reference(
    value: &format::Template,
    inputs: &[format::Input],
    kind: format::InputType,
) -> Option<String> {
    let name = value.reference()?;
    inputs
        .iter()
        .any(|input| input.name == name && input.kind == kind)
        .then(|| name.into())
}

pub(crate) fn scalar(value: &str, inputs: &[format::Input]) -> Result<Value<f32>, Error> {
    let parsed = template(value)?;
    if let Some(literal) = parsed.literal() {
        return literal
            .parse()
            .map(Value::Literal)
            .map_err(|_| Error(format!("invalid scalar {literal:?}")));
    }
    // A native scalar's display/parse round trip is the same f32 value. Avoid
    // allocating a string for this case without changing expansion semantics.
    if let Some(name) = typed_reference(&parsed, inputs, format::InputType::Scalar) {
        return Ok(Value::Input(name));
    }
    Ok(Value::Template(parsed))
}

pub(crate) fn color(value: &str, inputs: &[format::Input]) -> Result<Value<Color>, Error> {
    let parsed = template(value)?;
    if let Some(name) = typed_reference(&parsed, inputs, format::InputType::Color) {
        return Ok(Value::Input(name));
    }
    let Some(literal) = parsed.literal() else {
        return Ok(Value::Template(parsed));
    };
    let Some(hex) = literal
        .strip_prefix('#')
        .filter(|s| matches!(s.len(), 6 | 8) && s.bytes().all(|b| b.is_ascii_hexdigit()))
    else {
        return Err(Error(
            "color must expand to #RRGGBB or #RRGGBBAA, or reference a color input".into(),
        ));
    };
    let mut rgba = [255; 4];
    for (i, channel) in rgba.iter_mut().take(hex.len() / 2).enumerate() {
        *channel = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap();
    }
    Ok(Value::Literal(Color(rgba)))
}

fn color_field(value: &str, inputs: &[format::Input]) -> Result<ColorFieldExpr, Error> {
    let parsed = template(value)?;
    for kind in [format::InputType::ColorField, format::InputType::Canvas] {
        if let Some(name) = typed_reference(&parsed, inputs, kind) {
            return Ok(ColorFieldExpr::Input(name));
        }
    }
    color(value, inputs).map(ColorFieldExpr::Solid)
}

fn lower_node(node: &format::CanvasExpr, inputs: &[format::Input]) -> Result<CanvasExpr, Error> {
    Ok(match node {
        format::CanvasExpr::Get(_) => {
            return Err(Error("get must be resolved before lowering".into()));
        }
        format::CanvasExpr::Draw { source, coverage } => CanvasExpr::Draw {
            source: color_field(source, inputs)?,
            coverage: match coverage {
                format::CoverageExpr::All => CoverageExpr::All,
                format::CoverageExpr::Fill(path) => CoverageExpr::Fill(lower_path(path, inputs)?),
            },
        },
        format::CanvasExpr::Text {
            content,
            style,
            wrap_width,
        } => CanvasExpr::Text {
            content: lower_text(content, style, inputs)?,
            style: lower_style(style, inputs)?,
            wrap_width: wrap_width
                .as_deref()
                .map(|value| scalar(value, inputs))
                .transpose()?,
        },
        format::CanvasExpr::Translate { x, y, content } => CanvasExpr::Translate {
            x: scalar(x, inputs)?,
            y: scalar(y, inputs)?,
            content: Box::new(lower_node(content, inputs)?),
        },
        format::CanvasExpr::SourceOver {
            space,
            backdrop,
            source,
        } => CanvasExpr::SourceOver {
            space: match space.as_str() {
                "Srgb" => Space::Srgb,
                "LinearSrgb" => Space::LinearSrgb,
                "DisplayP3" => Space::DisplayP3,
                "LinearDisplayP3" => Space::LinearDisplayP3,
                _ => return Err(Error("unsupported composition space".into())),
            },
            backdrop: Box::new(lower_node(backdrop, inputs)?),
            source: Box::new(lower_node(source, inputs)?),
        },
        format::CanvasExpr::Reference(value) => CanvasExpr::Reference(
            template(value)?
                .reference()
                .ok_or_else(|| Error("Canvas reference requires a single {name} reference".into()))?
                .into(),
        ),
    })
}

pub(crate) fn lower_path(
    path: &format::PathExpr,
    inputs: &[format::Input],
) -> Result<PathExpr, Error> {
    Ok(match path {
        format::PathExpr::Get(_) => {
            return Err(Error("get must be resolved before lowering".into()));
        }
        format::PathExpr::Reference(reference) => PathExpr::Reference(
            template(reference)?
                .reference()
                .ok_or_else(|| Error("path requires a reference".into()))?
                .into(),
        ),
        format::PathExpr::Polygon(points) => PathExpr::Polygon(
            points
                .iter()
                .map(|point| Ok((scalar(&point.x, inputs)?, scalar(&point.y, inputs)?)))
                .collect::<Result<_, Error>>()?,
        ),
        format::PathExpr::Rect {
            x,
            y,
            width,
            height,
        } => PathExpr::Rect {
            x: scalar(x, inputs)?,
            y: scalar(y, inputs)?,
            width: scalar(width, inputs)?,
            height: scalar(height, inputs)?,
        },
    })
}

fn lower_style(style: &format::TextStyle, inputs: &[format::Input]) -> Result<TextStyle, Error> {
    Ok(TextStyle {
        fonts: template(&style.fonts)?
            .reference()
            .ok_or_else(|| Error("fonts requires a single {name} reference".into()))?
            .into(),
        font_size: scalar(&style.font_size, inputs)?,
        foreground: color_field(&style.foreground, inputs)?,
    })
}

fn lower_text(
    content: &[format::TextContent],
    parent: &format::TextStyle,
    inputs: &[format::Input],
) -> Result<Vec<crate::ir::TextContent>, Error> {
    let mut spans = Vec::new();
    for node in content {
        let (value, style) = match node {
            format::TextContent::String {
                template: value,
                style,
            } => (value.as_str(), style.resolve(parent)),
            format::TextContent::Break => ("\n", parent.clone()),
            format::TextContent::Span { style, children } => {
                let style = style.resolve(parent);
                spans.push(crate::ir::TextContent::Span {
                    children: lower_text(children, &style, inputs)?,
                    style: lower_style(&style, inputs)?,
                });
                continue;
            }
        };
        let parsed = template(value)?;
        let content = if let Some(literal) = parsed.literal() {
            Value::Literal(literal.into())
        } else if let Some(name) = typed_reference(&parsed, inputs, format::InputType::String) {
            Value::Input(name)
        } else {
            Value::Template(parsed)
        };
        spans.push(crate::ir::TextContent::String {
            content,
            style: lower_style(&style, inputs)?,
        });
    }
    Ok(spans)
}
