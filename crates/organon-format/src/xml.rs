use std::fmt::Write;

use roxmltree::Node as XmlNode;

use crate::{CanvasExpr, CoverageExpr, Error, PathExpr, Point, TextStyle};

pub(crate) fn at(node: XmlNode<'_, '_>, message: &str) -> Error {
    let position = node.document().text_pos_at(node.range().start);
    Error(format!("{}:{}: {message}", position.row, position.col))
}

// roxmltree itself recursively parses elements. Bound nesting before invoking
// it, while leaving XML validity to the parser. Quoted attributes, comments,
// processing instructions, and CDATA must not count as element boundaries.
pub(crate) fn check_source_depth(source: &str) -> Result<(), Error> {
    let mut rest = source;
    let mut depth = 0usize;
    while let Some(start) = rest.find('<') {
        rest = &rest[start..];
        let terminator = if rest.starts_with("<!--") {
            Some("-->")
        } else if rest.starts_with("<![CDATA[") {
            Some("]]>")
        } else if rest.starts_with("<?") {
            Some("?>")
        } else {
            None
        };
        if let Some(terminator) = terminator {
            let Some(end) = rest.find(terminator) else {
                break;
            };
            rest = &rest[end + terminator.len()..];
            continue;
        }
        if rest.starts_with("<!") {
            return Err(Error(
                "DTD and markup declarations are not supported".into(),
            ));
        }
        let mut quote = None;
        let mut end = None;
        for (i, byte) in rest.bytes().enumerate().skip(1) {
            match (quote, byte) {
                (Some(expected), actual) if expected == actual => quote = None,
                (None, b'\'' | b'"') => quote = Some(byte),
                (None, b'>') => {
                    end = Some(i);
                    break;
                }
                _ => (),
            }
        }
        let Some(end) = end else { break };
        if rest.starts_with("</") {
            depth = depth.saturating_sub(1);
        } else {
            // Element and Let plus 129 expression nodes (depth 0..=128).
            if depth >= 131 {
                return Err(Error("expression nesting exceeds 128 levels".into()));
            }
            if rest.as_bytes()[end - 1] != b'/' {
                depth += 1;
            }
        }
        rest = &rest[end + 1..];
    }
    Ok(())
}

pub(crate) fn attribute<'a>(node: XmlNode<'a, '_>, name: &str) -> Result<&'a str, Error> {
    node.attribute(name)
        .ok_or_else(|| at(node, &format!("missing attribute {name:?}")))
}

/// Store the same reference expression used by resource attributes.
pub(crate) fn parse_reference(node: XmlNode<'_, '_>) -> Result<String, Error> {
    check(node, "ref", &["name"])?;
    arity(node, 0)?;
    let name = attribute(node, "name")?;
    if !crate::template::identifier(name) {
        return Err(at(
            node,
            "ref name requires a fixed binding identifier; use get to select an output",
        ));
    }
    Ok(format!("{{{name}}}"))
}

pub(crate) fn write_reference(value: &str, depth: usize, output: &mut String) {
    let template = crate::Template::parse(value).expect("validated reference");
    let name = template.reference().expect("validated single reference");
    writeln!(output, "{}<ref name=\"{name}\"/>", "  ".repeat(depth)).unwrap();
}

pub(crate) fn check(node: XmlNode<'_, '_>, name: &str, attributes: &[&str]) -> Result<(), Error> {
    if node.tag_name().name() != name || node.tag_name().namespace().is_some() {
        return Err(at(node, &format!("expected {name}")));
    }
    for attribute in node.attributes() {
        if attribute.namespace().is_some() || !attributes.contains(&attribute.name()) {
            return Err(at(
                node,
                &format!("unknown attribute {:?}", attribute.name()),
            ));
        }
    }
    if node
        .children()
        .any(|child| child.is_text() && !child.text().unwrap_or_default().trim().is_empty())
    {
        return Err(at(node, "text content is not supported"));
    }
    Ok(())
}

pub(crate) fn arity<'a, 'input>(
    node: XmlNode<'a, 'input>,
    count: usize,
) -> Result<Vec<XmlNode<'a, 'input>>, Error> {
    let children: Vec<_> = node.children().filter(XmlNode::is_element).collect();
    if children.len() != count {
        return Err(at(node, &format!("expected {count} child elements")));
    }
    Ok(children)
}

pub(crate) fn parse_node(node: XmlNode<'_, '_>, depth: usize) -> Result<CanvasExpr, Error> {
    if depth > 128 {
        return Err(at(node, "drawing nesting exceeds 128 levels"));
    }
    Ok(match node.tag_name().name() {
        "get" => CanvasExpr::Get(crate::element::parse_get(node, depth)?),
        "let" => return Err(at(node, "let is only allowed directly under element")),
        "Draw" => {
            check(node, "Draw", &["source"])?;
            CanvasExpr::Draw {
                source: attribute(node, "source")?.into(),
                coverage: parse_coverage(arity(node, 1)?[0], depth + 1)?,
            }
        }
        "Text" => {
            check(
                node,
                "Text",
                &["fonts", "font_size", "foreground", "wrap_width"],
            )?;
            CanvasExpr::Text {
                content: node
                    .children()
                    .filter(XmlNode::is_element)
                    .map(|child| crate::text::parse(child, depth + 1))
                    .collect::<Result<_, _>>()?,
                style: TextStyle {
                    fonts: attribute(node, "fonts")?.into(),
                    font_size: attribute(node, "font_size")?.into(),
                    foreground: attribute(node, "foreground")?.into(),
                },
                wrap_width: node.attribute("wrap_width").map(str::to_owned),
            }
        }
        "Translate" => {
            check(node, "Translate", &["x", "y"])?;
            let children = arity(node, 1)?;
            CanvasExpr::Translate {
                x: attribute(node, "x")?.into(),
                y: attribute(node, "y")?.into(),
                content: Box::new(parse_node(children[0], depth + 1)?),
            }
        }
        "SourceOver" => {
            check(node, "SourceOver", &["space"])?;
            let children = arity(node, 2)?;
            CanvasExpr::SourceOver {
                space: attribute(node, "space")?.into(),
                backdrop: Box::new(parse_node(children[0], depth + 1)?),
                source: Box::new(parse_node(children[1], depth + 1)?),
            }
        }
        "ref" => CanvasExpr::Reference(parse_reference(node)?),
        _ => return Err(at(node, "unknown drawing element")),
    })
}

pub(crate) fn write_node(node: &CanvasExpr, depth: usize, output: &mut String) {
    let pad = "  ".repeat(depth);
    match node {
        CanvasExpr::Get(value) => crate::element::write_get(value, depth, output),
        CanvasExpr::Draw { source, coverage } => {
            writeln!(
                output,
                "{pad}<Draw source=\"{}\">",
                escape_attribute(source)
            )
            .unwrap();
            write_coverage(coverage, depth + 1, output);
            writeln!(output, "{pad}</Draw>").unwrap();
        }
        CanvasExpr::Text {
            content,
            style,
            wrap_width,
        } => {
            write!(
                output,
                "{pad}<Text fonts=\"{}\" font_size=\"{}\" foreground=\"{}\"",
                escape_attribute(&style.fonts),
                escape_attribute(&style.font_size),
                escape_attribute(&style.foreground)
            )
            .unwrap();
            if let Some(width) = wrap_width {
                write!(output, " wrap_width=\"{}\"", escape_attribute(width)).unwrap();
            }
            writeln!(output, ">").unwrap();
            for child in content {
                crate::text::write(child, depth + 1, output);
            }
            writeln!(output, "{pad}</Text>").unwrap();
        }
        CanvasExpr::Translate { x, y, content } => {
            writeln!(
                output,
                "{pad}<Translate x=\"{}\" y=\"{}\">",
                escape_attribute(x),
                escape_attribute(y)
            )
            .unwrap();
            write_node(content, depth + 1, output);
            writeln!(output, "{pad}</Translate>").unwrap();
        }
        CanvasExpr::SourceOver {
            space,
            backdrop,
            source,
        } => {
            writeln!(
                output,
                "{pad}<SourceOver space=\"{}\">",
                escape_attribute(space)
            )
            .unwrap();
            write_node(backdrop, depth + 1, output);
            write_node(source, depth + 1, output);
            writeln!(output, "{pad}</SourceOver>").unwrap();
        }
        CanvasExpr::Reference(name) => {
            write_reference(name, depth, output);
        }
    }
}

fn parse_coverage(node: XmlNode<'_, '_>, depth: usize) -> Result<CoverageExpr, Error> {
    match node.tag_name().name() {
        "let" => Err(at(node, "let is only allowed directly under element")),
        "All" => {
            check(node, "All", &[])?;
            arity(node, 0)?;
            Ok(CoverageExpr::All)
        }
        "Fill" => {
            check(node, "Fill", &[])?;
            let path = parse_path(arity(node, 1)?[0], depth + 1)?;
            Ok(CoverageExpr::Fill(path))
        }
        _ => Err(at(node, "expected a Coverage expression: All or Fill")),
    }
}

pub(crate) fn parse_path(node: XmlNode<'_, '_>, depth: usize) -> Result<PathExpr, Error> {
    if depth > 128 {
        return Err(at(node, "expression nesting exceeds 128 levels"));
    }
    match node.tag_name().name() {
        "get" => Ok(PathExpr::Get(crate::element::parse_get(node, depth)?)),
        "let" => Err(at(node, "let is only allowed directly under element")),
        "ref" => Ok(PathExpr::Reference(parse_reference(node)?)),
        "Polygon" => {
            check(node, "Polygon", &[])?;
            let mut points = Vec::new();
            for point in node.children().filter(XmlNode::is_element) {
                check(point, "Point", &["x", "y"])?;
                arity(point, 0)?;
                points.push(Point {
                    x: attribute(point, "x")?.into(),
                    y: attribute(point, "y")?.into(),
                });
            }
            Ok(PathExpr::Polygon(points))
        }
        "Rect" => {
            check(node, "Rect", &["x", "y", "width", "height"])?;
            arity(node, 0)?;
            Ok(PathExpr::Rect {
                x: attribute(node, "x")?.into(),
                y: attribute(node, "y")?.into(),
                width: attribute(node, "width")?.into(),
                height: attribute(node, "height")?.into(),
            })
        }
        _ => Err(at(
            node,
            "expected a Path expression: Polygon, Rect, or ref",
        )),
    }
}

fn write_coverage(coverage: &CoverageExpr, depth: usize, output: &mut String) {
    let pad = "  ".repeat(depth);
    match coverage {
        CoverageExpr::All => writeln!(output, "{pad}<All/>").unwrap(),
        CoverageExpr::Fill(path) => {
            writeln!(output, "{pad}<Fill>").unwrap();
            write_path(path, depth + 1, output);
            writeln!(output, "{pad}</Fill>").unwrap();
        }
    }
}

pub(crate) fn write_path(path: &PathExpr, depth: usize, output: &mut String) {
    let pad = "  ".repeat(depth);
    match path {
        PathExpr::Get(value) => crate::element::write_get(value, depth, output),
        PathExpr::Reference(value) => write_reference(value, depth, output),
        PathExpr::Polygon(points) => {
            writeln!(output, "{pad}<Polygon>").unwrap();
            for point in points {
                writeln!(
                    output,
                    "{pad}  <Point x=\"{}\" y=\"{}\"/>",
                    escape_attribute(&point.x),
                    escape_attribute(&point.y)
                )
                .unwrap();
            }
            writeln!(output, "{pad}</Polygon>").unwrap();
        }
        PathExpr::Rect {
            x,
            y,
            width,
            height,
        } => writeln!(
            output,
            "{pad}<Rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/>",
            escape_attribute(x),
            escape_attribute(y),
            escape_attribute(width),
            escape_attribute(height)
        )
        .unwrap(),
    }
}

pub(crate) fn escape_attribute(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('"', "&quot;")
        .replace('\n', "&#10;")
        .replace('\r', "&#13;")
        .replace('\t', "&#9;")
}
