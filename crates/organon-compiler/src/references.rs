//! Resolve named output projections without interpreting literal template text.
use crate::Error;
use organon_format::{
    CanvasExpr, CoverageExpr, PathExpr, Segment, Template, TextContent, TextStyleOverride,
    ValueExpr,
};

pub(crate) fn rewrite(
    value: &mut ValueExpr,
    resolve: &mut impl FnMut(&str) -> Result<String, Error>,
) -> Result<(), Error> {
    match value {
        ValueExpr::Get(_) => Ok(()),
        ValueExpr::Reference(v) => attribute(v, resolve),
        ValueExpr::Path(v) => path(v, resolve),
        ValueExpr::Canvas(v) => canvas(v, resolve),
        ValueExpr::Instance(v) => {
            for (_, value) in &mut v.inputs {
                attribute(value, resolve)?;
            }
            Ok(())
        }
    }
}
fn attribute(
    value: &mut String,
    resolve: &mut impl FnMut(&str) -> Result<String, Error>,
) -> Result<(), Error> {
    let template = Template::parse(value).map_err(|e| Error(e.to_string()))?;
    let mut text = String::new();
    for segment in template.segments() {
        match segment {
            Segment::Literal(v) => text.push_str(&v.replace('{', "{{").replace('}', "}}")),
            Segment::Reference(v) => {
                let name = resolve(v)?;
                text.push('{');
                text.push_str(&name);
                text.push('}');
            }
        }
    }
    *value = text;
    Ok(())
}
fn path(
    value: &mut PathExpr,
    resolve: &mut impl FnMut(&str) -> Result<String, Error>,
) -> Result<(), Error> {
    match value {
        PathExpr::Get(_) => return Err(Error("get must be resolved before lowering".into())),
        PathExpr::Reference(v) => attribute(v, resolve)?,
        PathExpr::Rect {
            x,
            y,
            width,
            height,
        } => {
            for v in [x, y, width, height] {
                attribute(v, resolve)?;
            }
        }
        PathExpr::Polygon(points) => {
            for p in points {
                attribute(&mut p.x, resolve)?;
                attribute(&mut p.y, resolve)?;
            }
        }
    }
    Ok(())
}
fn style(
    value: &mut TextStyleOverride,
    resolve: &mut impl FnMut(&str) -> Result<String, Error>,
) -> Result<(), Error> {
    for v in [
        &mut value.fonts,
        &mut value.font_size,
        &mut value.foreground,
    ]
    .into_iter()
    .flatten()
    {
        attribute(v, resolve)?;
    }
    Ok(())
}
fn text(
    value: &mut TextContent,
    resolve: &mut impl FnMut(&str) -> Result<String, Error>,
) -> Result<(), Error> {
    match value {
        TextContent::String { template, style: s } => {
            attribute(template, resolve)?;
            style(s, resolve)?;
        }
        TextContent::Span { children, style: s } => {
            style(s, resolve)?;
            for c in children {
                text(c, resolve)?;
            }
        }
        TextContent::Break => (),
    }
    Ok(())
}
fn canvas(
    value: &mut CanvasExpr,
    resolve: &mut impl FnMut(&str) -> Result<String, Error>,
) -> Result<(), Error> {
    match value {
        CanvasExpr::Get(_) => return Err(Error("get must be resolved before lowering".into())),
        CanvasExpr::Reference(v) => attribute(v, resolve)?,
        CanvasExpr::Draw { source, coverage } => {
            attribute(source, resolve)?;
            if let CoverageExpr::Fill(p) = coverage {
                path(p, resolve)?;
            }
        }
        CanvasExpr::Translate { x, y, content } => {
            attribute(x, resolve)?;
            attribute(y, resolve)?;
            canvas(content, resolve)?;
        }
        CanvasExpr::SourceOver {
            backdrop, source, ..
        } => {
            canvas(backdrop, resolve)?;
            canvas(source, resolve)?;
        }
        CanvasExpr::Text {
            content,
            style,
            wrap_width,
        } => {
            for v in [
                &mut style.fonts,
                &mut style.font_size,
                &mut style.foreground,
            ] {
                attribute(v, resolve)?;
            }
            if let Some(v) = wrap_width {
                attribute(v, resolve)?;
            }
            for child in content {
                text(child, resolve)?;
            }
        }
    }
    Ok(())
}
