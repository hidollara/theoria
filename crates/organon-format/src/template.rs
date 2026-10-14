use crate::Error;

/// Parsed attribute text. Expansion is a single pass; input values are not
/// themselves interpreted as templates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Template {
    segments: Vec<Segment>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Segment {
    Literal(String),
    Reference(String),
}

impl Template {
    /// Accepts `{name}`, `{{` and `}}`. Nested braces, empty references, Rust
    /// expressions, and format specifiers are not part of this grammar.
    pub fn parse(source: &str) -> Result<Self, Error> {
        crate::text(source)?;
        let mut chars = source.chars().peekable();
        let mut segments = Vec::new();
        let mut literal = String::new();
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
                        segments.push(Segment::Literal(std::mem::take(&mut literal)));
                    }
                    let mut name = String::new();
                    loop {
                        match chars.next() {
                            Some('}') => break,
                            Some('{') => {
                                return Err(Error(
                                    "nested attribute references are not supported".into(),
                                ));
                            }
                            Some(c) => name.push(c),
                            None => return Err(Error("unclosed attribute reference".into())),
                        }
                    }
                    if !reference_name(&name) {
                        return Err(Error(format!("invalid attribute reference {name:?}")));
                    }
                    segments.push(Segment::Reference(name));
                }
                '}' => {
                    return Err(Error(
                        "unmatched closing brace; use }} for a literal brace".into(),
                    ));
                }
                c => literal.push(c),
            }
        }
        if !literal.is_empty() || segments.is_empty() {
            segments.push(Segment::Literal(literal));
        }
        Ok(Self { segments })
    }

    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }

    /// The expanded literal, when no variable lookup is needed.
    pub fn literal(&self) -> Option<&str> {
        match self.segments.as_slice() {
            [Segment::Literal(value)] => Some(value),
            _ => None,
        }
    }

    /// A single reference, useful for attributes that select a native resource.
    pub fn reference(&self) -> Option<&str> {
        match self.segments.as_slice() {
            [Segment::Reference(name)] => Some(name),
            _ => None,
        }
    }
}

pub(crate) fn identifier(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub(crate) fn reference_name(name: &str) -> bool {
    name.split('.').all(identifier)
}
