//! App-owned XML generation. This is a fresh representation, not a source-preserving save.
use organon_core::{Error, syntax::*};
use std::fmt::Write;
mod branch;

pub fn write(value: &Module) -> Result<String, Error> {
    value.validate()?;
    let mut text = String::from("<organon version=\"1\">\n");
    for item in &value.items {
        let definition = match item {
            Item::Import(value) => {
                import(value, &mut text);
                continue;
            }
            Item::Use(value) => {
                selection(value, &mut text);
                continue;
            }
            Item::Export(value) => {
                export(value, &mut text);
                continue;
            }
            Item::Definition(value) => value,
        };
        match definition {
            Definition::Interface(value) => interface(value, &mut text),
            Definition::Predicate(p) => branch::predicate(p, &mut text),
            Definition::DeclarePredicate(p) => branch::declare_predicate(p, &mut text),
            Definition::Element(value) => {
                element(value, &mut text);
            }
            Definition::DeclareElement(value) => {
                declare_element(value, &mut text);
            }
        }
    }
    text.push_str("</organon>\n");
    if organon_core::xml::parse(&text)? != *value {
        return Err(Error(
            "definition cannot be represented by this XML writer".into(),
        ));
    }
    Ok(text)
}

fn provide_output(value: &Provide, out: &mut String, depth: usize) {
    writeln!(
        out,
        "{}<provide name=\"{}\" ref=\"{}\"/>",
        "  ".repeat(depth),
        escape_attribute(&value.name),
        escape_attribute(&value.reference)
    )
    .unwrap();
}

fn interface(value: &Interface, out: &mut String) {
    writeln!(
        out,
        "  <interface name=\"{}\">",
        escape_attribute(&value.name)
    )
    .unwrap();
    for port in &value.outputs {
        write_port(out, "output", port, "");
    }
    out.push_str("  </interface>\n");
}

fn implementation(value: &Implementation, out: &mut String, depth: usize) {
    let pad = "  ".repeat(depth);
    writeln!(
        out,
        "{pad}<impl interface=\"{}\">",
        escape_attribute(&value.interface)
    )
    .unwrap();
    for value in &value.provides {
        provide_output(value, out, depth + 1);
    }
    writeln!(out, "{pad}</impl>").unwrap();
}

fn element(value: &Element, xml: &mut String) {
    write!(xml, "  <element name=\"{}\"", escape_attribute(&value.name)).unwrap();
    write_children(value.children.as_ref(), xml);
    xml.push_str(">\n");
    for member in &value.members {
        match member {
            ElementMember::Impl(value) => implementation(value, xml, 2),
            ElementMember::Input(input) => write_port(xml, "input", input, ""),
            ElementMember::Binding(binding) => branch::binding(binding, 2, xml),
            ElementMember::Provide(provide) => provide_output(provide, xml, 2),
        }
    }
    xml.push_str("  </element>\n");
}

fn export(value: &Export, text: &mut String) {
    write!(text, "  <export name=\"{}\"", value.name).unwrap();
    if let Some(alias) = &value.alias {
        write!(text, " as=\"{}\"", alias).unwrap();
    }
    text.push_str("/>\n");
}

fn declare_element(value: &DeclareElement, text: &mut String) {
    write!(
        text,
        "  <declare-element name=\"{}\"{}",
        escape_attribute(&value.name),
        if value.tag { "" } else { " tag=\"false\"" }
    )
    .unwrap();
    write_children(value.children.as_ref(), text);
    text.push_str(">\n");
    for member in &value.members {
        match member {
            DeclareElementMember::Implements(name) => writeln!(
                text,
                "    <implements interface=\"{}\"/>",
                escape_attribute(name)
            )
            .unwrap(),
            DeclareElementMember::Input(port) => write_port(text, "input", port, ""),
            DeclareElementMember::OptionalInput(port) => {
                write_port(text, "input", port, " optional=\"true\"")
            }
            DeclareElementMember::Output(output) => write_port(text, "output", output, ""),
        }
    }
    text.push_str("  </declare-element>\n");
}

fn import(value: &Import, text: &mut String) {
    use std::fmt::Write;
    writeln!(
        text,
        "  <import from=\"{}\"/>",
        escape_attribute(value.from.as_str())
    )
    .unwrap();
}

fn selection(value: &Use, text: &mut String) {
    use std::fmt::Write;
    write!(
        text,
        "  <use from=\"{}\" name=\"{}\"",
        escape_attribute(value.from.as_str()),
        value.name
    )
    .unwrap();
    if let Some(alias) = &value.alias {
        write!(text, " as=\"{}\"", alias).unwrap();
    }
    text.push_str("/>\n");
}

fn write_let(value: &Let, depth: usize, xml: &mut String) {
    let pad = "  ".repeat(depth);
    write!(xml, "{pad}<let name=\"{}\"", escape_attribute(&value.name)).unwrap();
    xml.push_str(">\n");
    write_element_expression(&value.element, depth + 1, xml);
    writeln!(xml, "{pad}</let>").unwrap();
}

fn write_element_expression(value: &ElementExpression, depth: usize, xml: &mut String) {
    let pad = "  ".repeat(depth);
    match value {
        ElementExpression::Select(s) => branch::select(s, depth, xml),
        ElementExpression::Try(t) => branch::attempt(t, depth, xml),
        ElementExpression::Children => writeln!(xml, "{pad}<children/>").unwrap(),
        ElementExpression::Reference { name, ty } => {
            write!(xml, "{}<ref name=\"{name}\"", "  ".repeat(depth)).unwrap();
            if let Some(ty) = ty {
                write!(xml, " type=\"{}\"", ty.spelling()).unwrap();
            }
            xml.push_str("/>\n");
        }
        ElementExpression::Construct(value) => {
            write!(xml, "{pad}<{}", value.element).unwrap();
            for (name, value) in &value.arguments {
                write!(xml, " {name}=\"{}\"", escape_attribute(value)).unwrap();
            }
            if value.children.is_empty() {
                xml.push_str("/>\n");
            } else {
                xml.push_str(">\n");
                for child in &value.children {
                    write_element_expression(child, depth + 1, xml);
                }
                writeln!(xml, "{pad}</{}>", value.element).unwrap();
            }
        }
    }
}

fn write_bind(bind: &Bind, depth: usize, xml: &mut String) {
    let pad = "  ".repeat(depth);
    write!(xml, "{pad}<bind").unwrap();
    for (field, name) in &bind.outputs {
        write!(xml, " {field}=\"{}\"", escape_attribute(name)).unwrap();
    }
    xml.push_str(">\n");
    write_element_expression(&bind.element, depth + 1, xml);
    writeln!(xml, "{pad}</bind>").unwrap();
}

fn write_port(text: &mut String, tag: &str, port: &Port, attributes: &str) {
    write!(
        text,
        "    <{tag} name=\"{}\" type=\"{}\"{attributes}",
        port.name,
        port.ty.kind.spelling()
    )
    .unwrap();
    if port.ty.errors.is_empty() {
        text.push_str("/>\n");
    } else {
        text.push_str(">\n");
        for reason in &port.ty.errors {
            writeln!(text, "      <error type=\"{}\"/>", reason.spelling()).unwrap();
        }
        writeln!(text, "    </{tag}>").unwrap();
    }
}

fn write_children(children: Option<&ChildrenType>, out: &mut String) {
    if let Some(children) = children {
        write!(
            out,
            " child=\"{}{}\"",
            escape_attribute(children.kind.spelling()),
            match children.count {
                ChildCount::One => "",
                ChildCount::Optional => "?",
                ChildCount::Many => "*",
            }
        )
        .unwrap();
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
