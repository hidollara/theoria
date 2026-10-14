//! Template attribute grammar used by the Theoria string implementation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum TextPart {
    Literal(String),
    Reference(String),
}
pub(super) fn parse(source: &str) -> Result<Vec<TextPart>, String> {
    let mut chars = source.chars().peekable();
    let mut literal = String::new();
    let mut parts = vec![];
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                literal.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                literal.push('}');
            }
            '{' => {
                if !literal.is_empty() {
                    parts.push(TextPart::Literal(std::mem::take(&mut literal)));
                }
                let mut name = String::new();
                let mut closed = false;
                for c in chars.by_ref() {
                    if c == '}' {
                        closed = true;
                        break;
                    }
                    name.push(c);
                }
                if !closed {
                    return Err("unclosed attribute reference".into());
                }
                name.parse::<organon_core::names::BindingName>()
                    .map_err(|e| e.to_string())?;
                parts.push(TextPart::Reference(name));
            }
            '}' => return Err("unescaped closing brace".into()),
            c => literal.push(c),
        }
    }
    if !literal.is_empty() || parts.is_empty() {
        parts.push(TextPart::Literal(literal));
    }
    Ok(parts)
}

#[cfg(test)]
mod tests {
    use super::{TextPart, parse};

    #[test]
    fn literals_escaped_braces_and_multiple_references() {
        assert_eq!(
            parse("{{{left}}} + {right}").unwrap(),
            vec![
                TextPart::Literal("{".into()),
                TextPart::Reference("left".into()),
                TextPart::Literal("} + ".into()),
                TextPart::Reference("right".into()),
            ]
        );
        assert_eq!(parse("").unwrap(), vec![TextPart::Literal(String::new())]);
        assert_eq!(
            parse("a\nb\t").unwrap(),
            vec![TextPart::Literal("a\nb\t".into())]
        );
    }

    #[test]
    fn malformed_templates_are_rejected_by_the_library_parser() {
        for text in ["{", "}", "{}", "{a.b}", "{a{b}"] {
            assert!(parse(text).is_err(), "{text}");
        }
    }
}
