use super::*;
use roxmltree::Node;

pub(super) fn predicate(node: Node<'_, '_>) -> Result<Predicate, Error> {
    check(node, "predicate", &["name"])?;
    let members = node
        .children()
        .filter(Node::is_element)
        .map(|child| match child.tag_name().name() {
            "input" => input_port(child).map(PredicateMember::Input),
            "let" | "bind" => binding(child, 0).map(PredicateMember::Binding),
            _ => condition(child, 0).map(PredicateMember::Predicate),
        })
        .collect::<Result<_, _>>()?;
    Ok(Predicate {
        name: identifier_attribute(node, "name")?,
        members,
    })
}

pub(super) fn declare_predicate(node: Node<'_, '_>) -> Result<DeclarePredicate, Error> {
    check(node, "declare-predicate", &["name"])?;
    let mut members = Vec::new();
    for child in node.children().filter(Node::is_element) {
        if child.tag_name().name() == "error" {
            members.push(DeclarePredicateMember::Error(error_type(child)?));
        } else {
            members.push(DeclarePredicateMember::Input(input_port(child)?));
        }
    }
    Ok(DeclarePredicate {
        name: identifier_attribute(node, "name")?,
        members,
    })
}

fn optional_type(node: Node<'_, '_>, key: &str) -> Result<Option<TypeName>, Error> {
    node.attribute(key)
        .map(TypeName::parse)
        .transpose()
        .map_err(|e| error_at(node, &e.0))
}

pub(super) fn binding(node: Node<'_, '_>, depth: usize) -> Result<Binding, Error> {
    let tag = node.tag_name().name();
    if tag == "let" {
        check(node, "let", &["name"])?;
        return Ok(Binding::Let(Let {
            name: identifier_attribute(node, "name")?,
            element: parse_element_expression(arity(node, 1)?[0], depth)?,
        }));
    }
    let attrs: Vec<_> = node.attributes().map(|a| a.name()).collect();
    check(node, "bind", &attrs)?;
    Ok(Binding::Bind(Bind {
        outputs: node
            .attributes()
            .map(|a| {
                Ok((
                    parse_identifier(node, a.name())?,
                    parse_identifier(node, a.value())?,
                ))
            })
            .collect::<Result<_, Error>>()?,
        element: parse_element_expression(arity(node, 1)?[0], depth)?,
    }))
}

pub(super) fn condition(node: Node<'_, '_>, depth: usize) -> Result<PredicateExpression, Error> {
    if depth > 128 {
        return Err(error_at(node, "expression nesting exceeds 128 levels"));
    }
    let name = node.tag_name().name();
    match name {
        "all" | "any" => {
            check(node, name, &[])?;
            let children = node
                .children()
                .filter(Node::is_element)
                .map(|child| condition(child, depth + 1))
                .collect::<Result<_, _>>()?;
            Ok(if name == "all" {
                PredicateExpression::All(children)
            } else {
                PredicateExpression::Any(children)
            })
        }
        "not" => {
            check(node, name, &[])?;
            Ok(PredicateExpression::Not(Box::new(condition(
                arity(node, 1)?[0],
                depth + 1,
            )?)))
        }
        _ => {
            if crate::syntax::reserved_name(name) {
                return Err(error_at(node, "expected a predicate"));
            }
            let attrs: Vec<_> = node.attributes().map(|a| a.name()).collect();
            check(node, name, &attrs)?;
            arity(node, 0)?;
            Ok(PredicateExpression::Call {
                name: parse_identifier(node, name)?,
                arguments: node
                    .attributes()
                    .map(|a| Ok((parse_identifier(node, a.name())?, a.value().into())))
                    .collect::<Result<_, Error>>()?,
            })
        }
    }
}

fn branch(node: Node<'_, '_>, depth: usize) -> Result<Branch, Error> {
    let members = node
        .children()
        .filter(Node::is_element)
        .map(|child| match child.tag_name().name() {
            "let" | "bind" => binding(child, depth).map(BranchMember::Binding),
            _ => condition(child, depth).map(BranchMember::Predicate),
        })
        .collect::<Result<_, _>>()?;
    Ok(Branch {
        value: attribute(node, "value")?.into(),
        throw: optional_type(node, "throw")?,
        members,
    })
}

pub(super) fn select(node: Node<'_, '_>, depth: usize) -> Result<Select, Error> {
    check(node, "select", &["type"])?;
    let result_type = TypeName::parse(attribute(node, "type")?)?;
    let cases = node
        .children()
        .filter(Node::is_element)
        .map(|child| {
            check(child, "case", &["value", "throw"])?;
            branch(child, depth)
        })
        .collect::<Result<_, _>>()?;
    Ok(Select { result_type, cases })
}

pub(super) fn attempt(node: Node<'_, '_>, depth: usize) -> Result<Try, Error> {
    check(node, "try", &["type", "value"])?;
    let members = node
        .children()
        .filter(Node::is_element)
        .map(|child| match child.tag_name().name() {
            "let" | "bind" => binding(child, depth).map(TryMember::Binding),
            "catch" => {
                check(child, "catch", &["error", "capture", "value", "throw"])?;
                Ok(TryMember::Catch(Catch {
                    error: optional_type(child, "error")?,
                    capture: optional_identifier_attribute(child, "capture")?,
                    branch: branch(child, depth)?,
                }))
            }
            "unreachable" => {
                check(child, "unreachable", &["error"])?;
                arity(child, 0)?;
                Ok(TryMember::Unreachable {
                    error: optional_type(child, "error")?,
                })
            }
            _ => Err(error_at(child, "expected let, bind, catch or unreachable")),
        })
        .collect::<Result<_, _>>()?;
    Ok(Try {
        result_type: TypeName::parse(attribute(node, "type")?)?,
        value: attribute(node, "value")?.into(),
        members,
    })
}
