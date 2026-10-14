//! Read XML into shared definition data. This module has no serialization policy.
use crate::{
    Error,
    syntax::*,
    types::{Port, TypeName},
};
mod branch;
mod helpers;
use helpers::*;

/// Extract definition content from an already parsed Organon root.
/// Source layout and editing state remain with the caller.
pub fn read(root: roxmltree::Node<'_, '_>) -> Result<Module, Error> {
    check(root, "organon", &["version"])?;
    if attribute(root, "version")? != "1" {
        return Err(error_at(root, "unsupported organon version (expected 1)"));
    }
    let mut items = vec![];
    for node in root.children().filter(roxmltree::Node::is_element) {
        if node.tag_name().name() == "import" {
            items.push(Item::Import(import(node)?));
            continue;
        }
        if node.tag_name().name() == "export" {
            items.push(Item::Export(export(node)?));
            continue;
        }
        if node.tag_name().name() == "use" {
            items.push(Item::Use(selection(node)?));
            continue;
        }
        let definition = match node.tag_name().name() {
            "interface" => Definition::Interface(interface(node)?),
            "declare-element" => Definition::DeclareElement(declare_element(node)?),
            "predicate" => Definition::Predicate(branch::predicate(node)?),
            "declare-predicate" => Definition::DeclarePredicate(branch::declare_predicate(node)?),
            "element" => Definition::Element(element(node)?),
            _ => {
                return Err(error_at(
                    node,
                    "expected import, use, export, element, declare-element, predicate, declare-predicate, or interface",
                ));
            }
        };
        items.push(Item::Definition(definition));
    }
    let result = Module { items };
    result.validate()?;
    Ok(result)
}

/// Parse one complete Organon source.
pub fn parse(source: &str) -> Result<Module, Error> {
    let document = roxmltree::Document::parse(source).map_err(|e| Error(e.to_string()))?;
    read(document.root_element())
}

/// Parse a construction expression independently of a complete source.
pub fn parse_expression(source: &str) -> Result<ValueExpression, Error> {
    let document = roxmltree::Document::parse(source).map_err(|e| Error(e.to_string()))?;
    let value = parse_value(document.root_element(), 0)?;
    value.validate(0)?;
    Ok(value)
}

fn provide(node: roxmltree::Node<'_, '_>) -> Result<Provide, Error> {
    check(node, "provide", &["name", "ref"])?;
    arity(node, 0)?;
    Ok(Provide {
        name: identifier_attribute(node, "name")?,
        reference: identifier_attribute(node, "ref")?,
    })
}

fn interface(node: roxmltree::Node<'_, '_>) -> Result<Interface, Error> {
    check(node, "interface", &["name"])?;
    let mut outputs = Vec::new();
    for child in node.children().filter(roxmltree::Node::is_element) {
        check(child, "output", &["name", "type"])?;
        outputs.push(Port {
            name: identifier_attribute(child, "name")?,
            ty: value_type(child)?,
        });
    }
    let value = Interface {
        name: identifier_attribute(node, "name")?,
        outputs,
    };
    value.validate()?;
    Ok(value)
}

fn implementation(node: roxmltree::Node<'_, '_>) -> Result<Implementation, Error> {
    check(node, "impl", &["interface"])?;
    let mut provides = Vec::new();
    for child in node.children().filter(roxmltree::Node::is_element) {
        provides.push(provide(child)?);
    }
    Ok(Implementation {
        interface: identifier_attribute(node, "interface")?,
        provides,
    })
}

fn element(root: roxmltree::Node<'_, '_>) -> Result<Element, Error> {
    check(
        root,
        "element",
        &["name", "child", "min-children", "max-children"],
    )?;
    let mut result = Element {
        name: identifier_attribute(root, "name")?,
        children: parse_children(root)?,
        members: vec![],
    };
    for child in root.children().filter(roxmltree::Node::is_element) {
        match child.tag_name().name() {
            "impl" => result
                .members
                .push(ElementMember::Impl(implementation(child)?)),
            "input" => {
                result
                    .members
                    .push(ElementMember::Input(input_port(child)?));
            }
            "let" | "bind" => result.members.push(match branch::local(child, 0)? {
                LocalBinding::Let(value) => ElementMember::Let(value),
                LocalBinding::Bind(value) => ElementMember::Bind(value),
            }),
            "provide" => result.members.push(ElementMember::Provide(provide(child)?)),
            _ => {
                return Err(error_at(
                    child,
                    "expected input, let, bind, provide, or impl",
                ));
            }
        }
    }
    result
        .validate()
        .map_err(|e| error_at(root, &e.to_string()))?;
    Ok(result)
}

fn export(node: roxmltree::Node<'_, '_>) -> Result<Export, Error> {
    check(node, "export", &["name", "as"])?;
    arity(node, 0)?;
    Ok(Export {
        name: identifier_attribute(node, "name")?,
        alias: optional_identifier_attribute(node, "as")?,
    })
}

fn declare_element(node: roxmltree::Node<'_, '_>) -> Result<DeclareElement, Error> {
    check(
        node,
        "declare-element",
        &["name", "tag", "child", "min-children", "max-children"],
    )?;
    let mut value = DeclareElement {
        name: identifier_attribute(node, "name")?,
        tag: boolean(node, "tag", true)?,
        children: parse_children(node)?,
        members: vec![],
    };
    for child in node.children().filter(roxmltree::Node::is_element) {
        if child.tag_name().name() == "implements" {
            check(child, "implements", &["interface"])?;
            arity(child, 0)?;
            value
                .members
                .push(DeclareElementMember::Implements(identifier_attribute(
                    child,
                    "interface",
                )?));
            continue;
        }
        let output = child.tag_name().name() == "output";
        check(
            child,
            if output { "output" } else { "input" },
            if output {
                &["name", "type"]
            } else {
                &["name", "type", "optional"]
            },
        )?;
        let port = Port {
            name: identifier_attribute(child, "name")?,
            ty: value_type(child)?,
        };
        if output {
            value.members.push(DeclareElementMember::Output(port));
        } else {
            value.members.push(if boolean(child, "optional", false)? {
                DeclareElementMember::OptionalInput(port)
            } else {
                DeclareElementMember::Input(port)
            });
        }
    }
    value
        .validate()
        .map_err(|e| error_at(node, &e.to_string()))?;
    Ok(value)
}

fn import(node: roxmltree::Node<'_, '_>) -> Result<Import, Error> {
    check(node, "import", &["from"])?;
    arity(node, 0)?;
    Ok(Import {
        from: ImportSource::parse(attribute(node, "from")?)?,
    })
}

fn selection(node: roxmltree::Node<'_, '_>) -> Result<Use, Error> {
    check(node, "use", &["from", "name", "as"])?;
    arity(node, 0)?;
    let value = Use {
        from: ImportSource::parse(attribute(node, "from")?)?,
        name: identifier_attribute(node, "name")?,
        alias: optional_identifier_attribute(node, "as")?,
    };
    value.validate()?;
    Ok(value)
}

fn parse_value(child: roxmltree::Node<'_, '_>, depth: usize) -> Result<ValueExpression, Error> {
    if depth > 128 {
        return Err(error_at(child, "expression nesting exceeds 128 levels"));
    }
    let name = child.tag_name().name();
    if name == "select" {
        return branch::select(child, depth + 1).map(ValueExpression::Select);
    }
    if name == "try" {
        return branch::attempt(child, depth + 1).map(ValueExpression::Try);
    }
    if name == "children" {
        check(child, "children", &[])?;
        arity(child, 0)?;
        return Ok(ValueExpression::Children);
    }
    if name == "ref" {
        return parse_reference(child);
    }
    if crate::syntax::reserved_name(name) {
        return Err(error_at(child, "expected an element expression"));
    }
    let attributes: Vec<_> = child.attributes().map(|a| a.name()).collect();
    check(child, name, &attributes)?;
    Ok(ValueExpression::Construct(Construction {
        element: parse_identifier(child, name)?,
        arguments: child
            .attributes()
            .map(|a| Ok((parse_identifier(child, a.name())?, a.value().into())))
            .collect::<Result<_, Error>>()?,
        children: child
            .children()
            .filter(roxmltree::Node::is_element)
            .map(|c| parse_value(c, depth + 1))
            .collect::<Result<_, _>>()?,
    }))
}
