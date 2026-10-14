use super::Error;
use roxmltree::Node as XmlNode;
use std::fmt::Write;
pub(super) fn error_at(node: XmlNode<'_, '_>, message: &str) -> Error {
    let position = node.document().text_pos_at(node.range().start);
    Error(format!("{}:{}: {message}", position.row, position.col))
}

pub(super) fn attribute<'a>(node: XmlNode<'a, '_>, name: &str) -> Result<&'a str, Error> {
    node.attribute(name)
        .ok_or_else(|| error_at(node, &format!("missing attribute {name:?}")))
}

/// Store a reference as its binding name.
pub(super) fn parse_reference(node: XmlNode<'_, '_>) -> Result<String, Error> {
    check(node, "ref", &["name"])?;
    arity(node, 0)?;
    let name = attribute(node, "name")?;
    if !crate::names::identifier(name) {
        return Err(error_at(
            node,
            "ref name requires a fixed binding identifier; use bind to select an output",
        ));
    }
    Ok(name.into())
}

pub(super) fn write_reference(name: &str, depth: usize, output: &mut String) {
    writeln!(output, "{}<ref name=\"{name}\"/>", "  ".repeat(depth)).unwrap();
}

pub(super) fn check(node: XmlNode<'_, '_>, name: &str, attributes: &[&str]) -> Result<(), Error> {
    if node.tag_name().name() != name || node.tag_name().namespace().is_some() {
        return Err(error_at(node, &format!("expected {name}")));
    }
    for attribute in node.attributes() {
        if attribute.namespace().is_some() || !attributes.contains(&attribute.name()) {
            return Err(error_at(
                node,
                &format!("unknown attribute {:?}", attribute.name()),
            ));
        }
    }
    if node
        .children()
        .any(|child| child.is_text() && !child.text().unwrap_or_default().trim().is_empty())
    {
        return Err(error_at(node, "text content is not supported"));
    }
    Ok(())
}

pub(super) fn arity<'a, 'input>(
    node: XmlNode<'a, 'input>,
    count: usize,
) -> Result<Vec<XmlNode<'a, 'input>>, Error> {
    let children: Vec<_> = node.children().filter(XmlNode::is_element).collect();
    if children.len() != count {
        return Err(error_at(node, &format!("expected {count} child elements")));
    }
    Ok(children)
}

pub(super) fn escape_attribute(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('"', "&quot;")
        .replace('\n', "&#10;")
        .replace('\r', "&#13;")
        .replace('\t', "&#9;")
}

/// Interpret a port's type name and independent fallibility flag.
pub(super) fn value_type(node: XmlNode<'_, '_>) -> Result<super::ValueType, Error> {
    let fallible = boolean(node, "fallible", false)?;
    super::ValueType::parse(attribute(node, "type")?, fallible)
        .map_err(|e| error_at(node, &e.to_string()))
}

pub(super) fn text(value: &str) -> Result<(), Error> {
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

pub(super) fn boolean(node: XmlNode<'_, '_>, name: &str, default: bool) -> Result<bool, Error> {
    match node.attribute(name) {
        None => Ok(default),
        Some("true") => Ok(true),
        Some("false") => Ok(false),
        _ => Err(error_at(
            node,
            &format!("{name} requires a fixed true or false"),
        )),
    }
}
