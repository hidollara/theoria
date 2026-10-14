use crate::Error;
use roxmltree::Node as XmlNode;

pub(super) fn parse_children(
    node: XmlNode<'_, '_>,
) -> Result<Option<crate::types::ChildrenType>, Error> {
    let Some(kind) = node.attribute("child") else {
        if node.has_attribute("min-children") || node.has_attribute("max-children") {
            return Err(error_at(node, "child counts require a child type"));
        }
        return Ok(None);
    };
    let number = |key| {
        node.attribute(key)
            .map(|s| {
                if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(error_at(node, "invalid child count"));
                }
                s.parse::<usize>()
                    .map_err(|_| error_at(node, "invalid child count"))
            })
            .transpose()
    };
    let children = crate::types::ChildrenType {
        kind: crate::types::TypeName::parse(kind).map_err(|e| error_at(node, &e.to_string()))?,
        minimum: number("min-children")?.unwrap_or(0),
        maximum: number("max-children")?,
    };
    children
        .validate()
        .map_err(|e| error_at(node, &e.to_string()))?;
    Ok(Some(children))
}

pub(super) fn error_at(node: XmlNode<'_, '_>, message: &str) -> Error {
    let position = node.document().text_pos_at(node.range().start);
    Error(format!("{}:{}: {message}", position.row, position.col))
}

pub(super) fn attribute<'a>(node: XmlNode<'a, '_>, name: &str) -> Result<&'a str, Error> {
    node.attribute(name)
        .ok_or_else(|| error_at(node, &format!("missing attribute {name:?}")))
}

pub(super) fn parse_reference(
    node: XmlNode<'_, '_>,
) -> Result<crate::syntax::ValueExpression, Error> {
    check(node, "ref", &["name", "type"])?;
    arity(node, 0)?;
    let name = attribute(node, "name")?.parse().map_err(|_: Error| {
        error_at(
            node,
            "ref name requires a fixed binding identifier; use bind to select an output",
        )
    })?;
    let ty = node
        .attribute("type")
        .map(crate::types::TypeName::parse)
        .transpose()
        .map_err(|e| error_at(node, &e.to_string()))?;
    Ok(crate::syntax::ValueExpression::Reference { name, ty })
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

pub(super) fn error_type(node: XmlNode<'_, '_>) -> Result<crate::types::TypeName, Error> {
    check(node, "error", &["type"])?;
    arity(node, 0)?;
    crate::types::TypeName::parse(attribute(node, "type")?).map_err(|e| error_at(node, &e.0))
}

pub(super) fn input_port(node: XmlNode<'_, '_>) -> Result<crate::types::Port, Error> {
    check(node, "input", &["name", "type"])?;
    Ok(crate::types::Port {
        name: identifier_attribute(node, "name")?,
        ty: value_type(node)?,
    })
}

pub(super) fn value_type(node: XmlNode<'_, '_>) -> Result<crate::types::ValueType, Error> {
    let kind = crate::types::TypeName::parse(attribute(node, "type")?)
        .map_err(|e| error_at(node, &e.0))?;
    let errors = node
        .children()
        .filter(XmlNode::is_element)
        .map(error_type)
        .collect::<Result<_, _>>()?;
    Ok(crate::types::ValueType { kind, errors })
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

pub(super) fn parse_identifier(
    node: XmlNode<'_, '_>,
    value: &str,
) -> Result<crate::identifier::Identifier, Error> {
    value.parse().map_err(|e: Error| error_at(node, &e.0))
}
pub(super) fn identifier_attribute(
    node: XmlNode<'_, '_>,
    key: &str,
) -> Result<crate::identifier::Identifier, Error> {
    parse_identifier(node, attribute(node, key)?)
}
pub(super) fn optional_identifier_attribute(
    node: XmlNode<'_, '_>,
    key: &str,
) -> Result<Option<crate::identifier::Identifier>, Error> {
    node.attribute(key)
        .map(|value| parse_identifier(node, value))
        .transpose()
}
