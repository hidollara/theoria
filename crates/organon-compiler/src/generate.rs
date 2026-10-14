use organon_format::{Segment, Template};
use std::fmt::Write;

use super::ir::{ColorFieldExpr, CoverageExpr, PathExpr};
use super::{CanvasExpr, Color, Space, Value};

pub(crate) fn scalar(value: &Value<f32>) -> String {
    match value {
        Value::Literal(value) => format!("{value:?}_f32"),
        Value::Input(name) => format!("*binding_{name}"),
        Value::Template(value) => format!(
            "{}.parse::<f32>().map_err(|_| \"attribute did not expand to a scalar\")?",
            expand(value)
        ),
    }
}

pub(crate) fn expand(template: &Template) -> String {
    if let Some(name) = template.reference() {
        return format!("::std::string::ToString::to_string(&binding_{name})");
    }
    let mut pattern = String::new();
    let mut arguments = String::new();
    for segment in template.segments() {
        match segment {
            Segment::Literal(value) => {
                pattern.push_str(&value.replace('{', "{{").replace('}', "}}"))
            }
            Segment::Reference(name) => {
                pattern.push_str("{}");
                write!(arguments, ", binding_{name}").unwrap();
            }
        }
    }
    format!("::std::format!({pattern:?}{arguments})")
}

pub(crate) fn color(value: &Value<Color>) -> String {
    match value {
        Value::Literal(Color([r, g, b, a])) => format!(
            "::theoria_canvas::Color::new(::theoria_canvas::color::Srgb, [{r}_u8, {g}, {b}], ::theoria_canvas::Opacity::<u8>::from({a}_u8))"
        ),
        Value::Input(name) => format!("binding_{name}.clone()"),
        Value::Template(value) => {
            let value = expand(value);
            format!(
                r##"{{
let encoded = {value};
let hex = encoded.strip_prefix('#')
    .filter(|s| matches!(s.len(), 6 | 8) && s.bytes().all(|b| b.is_ascii_hexdigit()))
    .ok_or("color must expand to #RRGGBB or #RRGGBBAA")?;
let mut rgba = [255_u8; 4];
for (i, channel) in rgba.iter_mut().take(hex.len() / 2).enumerate() {{
    *channel = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16)
        .map_err(|_| "invalid expanded color")?;
}}
::theoria_canvas::Color::new(::theoria_canvas::color::Srgb, [rgba[0], rgba[1], rgba[2]], ::theoria_canvas::Opacity::<u8>::from(rgba[3]))
}}"##
            )
        }
    }
}

pub(crate) fn generate_node(node: &CanvasExpr, next: &mut usize, output: &mut String) -> String {
    let id = *next;
    *next += 1;
    let name = format!("canvas_{id}");
    match node {
        CanvasExpr::Draw { source, coverage } => {
            let source = color_field(source);
            let coverage = match coverage {
                CoverageExpr::All => "::theoria_canvas::Coverage::all()".into(),
                CoverageExpr::Fill(path) => {
                    format!("::theoria_canvas::Coverage::fill({})", generate_path(path))
                }
            };
            writeln!(
                output,
                "let {name} = ::theoria_canvas::Canvas::draw({source}, {coverage});"
            )
            .unwrap();
        }
        CanvasExpr::Text {
            content,
            style,
            wrap_width,
        } => {
            let fonts = &style.fonts;
            let font_size = scalar(&style.font_size);
            let fill = color_field(&style.foreground);
            let transparent = color(&Value::Literal(Color([0; 4])));
            writeln!(
                output,
                "let {name} = {{
let text_fill = {fill};
let span_style = ::theoria_text::text::TextStyle::new(binding_{fonts}, {font_size}).with_foreground(text_fill.clone());
let text = ::theoria_text::text::Text::horizontal(span_style.clone());"
            )
            .unwrap();
            generate_text(content, style, output);
            if let Some(width) = wrap_width {
                writeln!(output, "let text = text.wrap_width({});", scalar(width)).unwrap();
            }
            writeln!(output, "let text = text.layout().map_err(|_| \"Text layout failed: check fonts, font size, and wrap width\")?;
text.to_canvas(text_fill).map_err(|_| \"Text glyph realization failed\")?
    .unwrap_or_else(|| ::theoria_canvas::Canvas::draw({transparent}, ::theoria_canvas::Coverage::all()))
}};").unwrap();
        }
        CanvasExpr::Translate { x, y, content } => {
            let child = generate_node(content, next, output);
            writeln!(output, "let {name} = {child}.transformed(::theoria_canvas::transform::Affine2::from_translation(::theoria_canvas::geometry::Offset::new({}, {})).ok_or(\"Translate requires finite offsets\")?);", scalar(x), scalar(y)).unwrap();
        }
        CanvasExpr::SourceOver {
            space,
            backdrop,
            source,
        } => {
            let backdrop = generate_node(backdrop, next, output);
            let source = generate_node(source, next, output);
            let space = match space {
                Space::Srgb => "Srgb",
                Space::LinearSrgb => "LinearSrgb",
                Space::DisplayP3 => "DisplayP3",
                Space::LinearDisplayP3 => "LinearDisplayP3",
            };
            writeln!(output, "let {name} = {backdrop}.composite({source}, ::theoria_canvas::Composition::new(::theoria_canvas::Composite::SourceOver, ::theoria_canvas::color::{space}));").unwrap();
        }
        CanvasExpr::Reference(input) => {
            writeln!(output, "let {name} = binding_{input}.clone();").unwrap();
        }
    }
    name
}

fn color_field(value: &ColorFieldExpr) -> String {
    let expression = match value {
        ColorFieldExpr::Solid(value) => color(value),
        ColorFieldExpr::Input(name) => format!("binding_{name}.clone()"),
    };
    format!("::theoria_canvas::ColorField::from({expression})")
}

pub(crate) fn generate_path(path: &PathExpr) -> String {
    let expression = match path {
        PathExpr::Reference(name) => format!("binding_{name}.clone()"),
        PathExpr::Polygon(points) => {
            let points = points
                .iter()
                .map(|(x, y)| {
                    format!(
                        "::theoria_canvas::geometry::Point::new({}, {})",
                        scalar(x),
                        scalar(y)
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("::theoria_canvas::path::Path::polygon([{points}])")
        }
        PathExpr::Rect {
            x,
            y,
            width,
            height,
        } => {
            let (x, y, width, height) = (scalar(x), scalar(y), scalar(width), scalar(height));
            format!(
                r#"{{
let (x, y, width, height) = ({x}, {y}, {width}, {height});
let (right, bottom) = (x + width, y + height);
if ![x, y, width, height, right, bottom].iter().all(|v| v.is_finite()) || width < 0.0 || height < 0.0 {{ return Err("Rect requires finite coordinates and nonnegative dimensions"); }}
::theoria_canvas::path::Path::polygon([
::theoria_canvas::geometry::Point::new(x, y),
::theoria_canvas::geometry::Point::new(right, y),
::theoria_canvas::geometry::Point::new(right, bottom),
::theoria_canvas::geometry::Point::new(x, bottom)])
}}"#
            )
        }
    };
    format!(
        "{{ let path = {expression}; if !path.is_finite() {{ return Err(\"Path coordinates must be finite\"); }} path }}"
    )
}

fn generate_text(
    nodes: &[crate::ir::TextContent],
    parent: &crate::ir::TextStyle,
    output: &mut String,
) {
    use crate::ir::TextContent;
    for node in nodes {
        let style = match node {
            TextContent::String { style, .. } | TextContent::Span { style, .. } => style,
        };
        writeln!(output, "let text = {{").unwrap();
        let mut expression = "span_style.clone()".to_owned();
        if style.fonts != parent.fonts {
            write!(expression, ".with_fonts(binding_{})", style.fonts).unwrap();
        }
        if style.font_size != parent.font_size {
            writeln!(
                output,
                "let size = {};
if !size.is_finite() || size < 0.0 {{ return Err(\"font size must be finite and nonnegative\"); }}",
                scalar(&style.font_size)
            )
            .unwrap();
            expression.push_str(".with_font_size(size)");
        }
        if style.foreground != parent.foreground {
            write!(
                expression,
                ".with_foreground({})",
                color_field(&style.foreground)
            )
            .unwrap();
        }
        writeln!(output, "let span_style = {expression};").unwrap();
        match node {
            TextContent::String { content, .. } => {
                let content = match content {
                    Value::Literal(text) => format!("{text:?}"),
                    Value::Input(input) => format!("binding_{input}.as_str()"),
                    Value::Template(value) => expand(value),
                };
                writeln!(
                    output,
                    "text.push(::theoria_text::text::Span::new({content}).with_style(span_style))"
                )
                .unwrap();
            }
            TextContent::Span { children, .. } => {
                writeln!(output, "let _ = &span_style;").unwrap();
                generate_text(children, style, output);
                writeln!(output, "text").unwrap();
            }
        }
        writeln!(output, "}};").unwrap();
    }
}
