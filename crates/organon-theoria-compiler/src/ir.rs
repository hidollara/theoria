//! Internal Rust construction plan lowered from the checked core IR.
use crate::types::{Port, Type};

/// A checked construction graph, independent of XML declaration structure.
#[derive(Clone, Debug)]
pub struct Element {
    pub body: Body,
    pub implementations: Vec<Implementation>,
    pub name: String,
    pub inputs: Vec<Port>,
    pub outputs: Vec<Output>,
}
/// Both construction strategies retain structured data until code generation.
#[derive(Clone, Debug)]
pub enum Body {
    Bindings(Vec<Binding>),
    Scoped(organon_core::semantic::Body),
}

impl Element {
    pub fn inputs(&self) -> impl Iterator<Item = &Port> {
        self.inputs.iter()
    }
    pub fn outputs(&self) -> impl Iterator<Item = &Output> {
        self.outputs.iter()
    }
}
#[derive(Clone, Debug)]
pub struct Output {
    pub name: String,
    pub binding: String,
    pub ty: Type,
}
#[derive(Clone, Debug)]
pub struct DeclareElement {
    pub implements: Vec<String>,
    pub name: String,
    pub inputs: Vec<Port>,
    pub outputs: Vec<Port>,
}
impl DeclareElement {
    pub fn inputs(&self) -> impl Iterator<Item = &Port> {
        self.inputs.iter()
    }
    pub fn outputs(&self) -> impl Iterator<Item = &Port> {
        self.outputs.iter()
    }
}
#[derive(Clone, Debug)]
pub struct Binding {
    pub name: String,
    pub ty: Type,
    pub references: Vec<String>,
    pub value: Expression,
}
#[derive(Clone, Debug)]
pub enum Expression {
    Native {
        expression: String,
    },
    Reference(String),
    Instance {
        element: String,
        arguments: Arguments,
    },
    Projection {
        parent: String,
        output: String,
    },
}
#[derive(Clone, Debug)]
pub enum Argument {
    Interpreted {
        marker: String,
        text: String,
        optional: bool,
    },
    Native {
        expression: String,
        references: Vec<String>,
    },
    Reference {
        name: String,
    },
}
pub type Arguments = std::collections::BTreeMap<String, Argument>;
impl Argument {
    pub fn references(&self) -> Vec<String> {
        match self {
            Self::Interpreted { .. } => vec![],
            Self::Native { references, .. } => references.clone(),
            Self::Reference { name, .. } => vec![name.clone()],
        }
    }
}
#[derive(Clone, Debug)]
pub struct Interface {
    pub name: String,
    pub outputs: Vec<Port>,
}
#[derive(Clone, Debug)]
pub struct Implementation {
    pub interface: String,
    pub outputs: Vec<Output>,
}

#[derive(Clone, Debug)]
pub enum Definition {
    Interface(Interface),
    Element(Element),
    DeclareElement(DeclareElement),
}
impl Definition {
    pub fn name(&self) -> &str {
        match self {
            Self::Element(e) => &e.name,
            Self::Interface(i) => &i.name,
            Self::DeclareElement(i) => &i.name,
        }
    }
}
#[derive(Clone, Debug)]
pub struct Program {
    /// Definitions in lowering order. No XML/storage tree is retained.
    pub definitions: Vec<Definition>,
}
impl Program {
    pub fn definitions(&self) -> impl Iterator<Item = &Definition> {
        self.definitions.iter()
    }
    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.definitions
            .iter()
            .filter_map(|definition| match definition {
                Definition::Element(element) => Some(element),
                _ => None,
            })
    }
}
