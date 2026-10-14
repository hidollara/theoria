//! Internal Rust construction plan lowered from the checked core IR.
use crate::types::{Input, Port, ValueType};

/// A checked construction graph, independent of XML declaration structure.
#[derive(Clone, Debug)]
pub struct Element {
    pub implementations: Vec<Implementation>,
    pub name: String,
    pub inputs: Vec<Input>,
    pub outputs: Vec<Output>,
    pub bindings: Vec<Binding>,
}
impl Element {
    pub fn inputs(&self) -> impl Iterator<Item = &Input> {
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
    pub ty: ValueType,
}
impl Output {
    pub fn public_name(&self) -> &str {
        &self.name
    }
}
#[derive(Clone, Debug)]
pub struct External {
    pub implements: Vec<String>,
    pub name: String,
    pub inputs: Vec<Port>,
    pub outputs: Vec<DeclaredOutput>,
}
#[derive(Clone, Debug)]
pub struct DeclaredOutput {
    pub port: Port,
    pub inline: bool,
}
impl External {
    pub fn inputs(&self) -> impl Iterator<Item = &Port> {
        self.inputs.iter()
    }
    pub fn outputs(&self) -> impl Iterator<Item = &DeclaredOutput> {
        self.outputs.iter()
    }
}
#[derive(Clone, Debug)]
pub struct Binding {
    pub name: String,
    pub ty: ValueType,
    pub inline: bool,
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
        output_type: ValueType,
        parent_fallible: bool,
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
    Coerce {
        name: String,
        interface: String,
        source_fallible: bool,
        target_fallible: bool,
    },
    Reference {
        name: String,
        lift_result: bool,
    },
}
pub type Arguments = std::collections::BTreeMap<String, Argument>;
impl Argument {
    pub fn references(&self) -> Vec<String> {
        match self {
            Self::Interpreted { .. } => vec![],
            Self::Native { references, .. } => references.clone(),
            Self::Coerce { name, .. } | Self::Reference { name, .. } => vec![name.clone()],
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
    External(External),
}
impl Definition {
    pub fn name(&self) -> &str {
        match self {
            Self::Element(e) => &e.name,
            Self::Interface(i) => &i.name,
            Self::External(i) => &i.name,
        }
    }
}
#[derive(Clone, Debug)]
pub struct Program {
    /// Definitions in authored order. No XML/storage tree is retained.
    pub definitions: Vec<Definition>,
    /// Definition indices in construction order (interfaces have no construction body).
    pub construction_order: Vec<usize>,
}
impl Program {
    pub fn definitions(&self) -> impl Iterator<Item = &Definition> {
        self.definitions.iter()
    }
    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.construction_order
            .iter()
            .map(|&i| match &self.definitions[i] {
                Definition::Element(e) => e,
                _ => unreachable!(),
            })
    }
}
