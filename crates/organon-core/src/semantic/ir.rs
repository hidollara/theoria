use crate::types::{Input, Port, ValueType};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub struct Signature {
    pub inline_outputs: BTreeSet<String>,
    pub kind: DefinitionKind,
    pub inputs: Vec<Port>,
    pub optional_inputs: BTreeSet<String>,
    pub children: Option<crate::types::Children>,
    pub outputs: Vec<Port>,
    pub implements: Vec<String>,
}
/// How a definition is supplied; independent of Rust types and implementation paths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DefinitionKind {
    Element,
    External { tag: bool },
    Interface,
}
impl Signature {
    pub fn constructible(&self) -> bool {
        matches!(
            self.kind,
            DefinitionKind::Element | DefinitionKind::External { tag: true }
        )
    }
}
#[derive(Clone, Debug)]
pub struct Program {
    pub signatures: BTreeMap<String, Signature>,
    /// Dependency order; source declaration order remains in the format tree.
    pub elements: Vec<Element>,
}
#[derive(Clone, Debug)]
pub struct Element {
    pub name: String,
    pub inputs: Vec<Input>,
    pub bindings: Vec<Binding>,
    pub outputs: Vec<Output>,
    pub implementations: Vec<Implementation>,
}
#[derive(Clone, Debug)]
pub struct Binding {
    pub name: String,
    pub origin: usize,
    pub inline: bool,
    pub value: Expression,
}
#[derive(Clone, Debug)]
pub struct Output {
    pub name: String,
    pub binding: String,
    pub ty: ValueType,
}
#[derive(Clone, Debug)]
pub struct Implementation {
    pub interface: String,
    pub outputs: Vec<Output>,
}
#[derive(Clone, Debug)]
pub struct Expression {
    pub ty: ValueType,
    pub kind: ExpressionKind,
}
#[derive(Clone, Debug)]
pub enum ExpressionKind {
    Reference(String),
    Construct {
        name: String,
        arguments: BTreeMap<String, Argument>,
        children: Vec<Expression>,
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
    Missing {
        expected: ValueType,
    },
    Reference {
        name: String,
        expected: ValueType,
    },
    /// Decoded attribute text, interpreted later by the receiving native type.
    Attribute {
        text: String,
        expected: ValueType,
    },
}
