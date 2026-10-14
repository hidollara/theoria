use super::{ChildrenContract, Port, Type, TypeContract};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
/// Checked inputs, outputs and conformance shared by all definition kinds.
pub struct Contract {
    pub kind: DefinitionKind,
    pub inputs: Vec<Port>,
    pub optional_inputs: BTreeSet<String>,
    pub children: Option<ChildrenContract>,
    pub outputs: Vec<Port<OutputType>>,
    pub implements: Vec<String>,
}
/// Declared output contracts and outputs inferred from an Element's implementation.
/// Inferred errors live only in the effects graph, never in a partial TypeContract.
#[derive(Clone, Debug)]
pub enum OutputType {
    Declared(TypeContract),
    Inferred(Type),
}
impl OutputType {
    pub fn kind(&self) -> &Type {
        match self {
            Self::Declared(v) => &v.kind,
            Self::Inferred(v) => v,
        }
    }
    pub fn declared(&self) -> Option<&TypeContract> {
        match self {
            Self::Declared(v) => Some(v),
            Self::Inferred(_) => None,
        }
    }
}

/// How a definition is supplied; independent of Rust types and implementation paths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DefinitionKind {
    Element,
    DeclareElement { tag: bool },
    Interface,
}
impl Contract {
    pub fn constructible(&self) -> bool {
        matches!(
            self.kind,
            DefinitionKind::Element | DefinitionKind::DeclareElement { tag: true }
        )
    }
}
#[derive(Clone, Debug)]
pub struct Program {
    pub predicates: BTreeMap<String, Predicate>,
    pub contracts: BTreeMap<String, Contract>,
    /// Dependency order; source declaration order remains in `syntax::Module`.
    pub bodies: Vec<Body>,
}
#[derive(Clone, Debug)]
/// Construction implementation of an Element. Its contract is stored once in Program.
pub struct Body {
    pub name: String,
    pub bindings: Vec<Binding>,
    pub outputs: Vec<OutputBinding>,
    pub implementations: Vec<Implementation>,
}
#[derive(Clone, Debug)]
pub struct Binding {
    pub name: String,
    pub origin: usize,
    pub value: Expression,
}
#[derive(Clone, Debug)]
/// Bind a contract's output to an internal value; the type belongs to the contract.
pub struct OutputBinding {
    pub name: String,
    pub binding: String,
}
#[derive(Clone, Debug)]
pub struct Implementation {
    pub interface: String,
    pub outputs: Vec<OutputBinding>,
}
#[derive(Clone, Debug)]
pub struct Expression {
    pub ty: Type,
    pub kind: ExpressionKind,
}
#[derive(Clone, Debug)]
pub enum ExpressionKind {
    Select(Vec<Branch>),
    Try {
        body: Block,
        handlers: Vec<Handler>,
    },
    Reference(String),
    /// View a binding through its declared implementation of the expression's result type.
    AsInterface(String),
    Construct {
        name: String,
        arguments: BTreeMap<String, Argument>,
        children: Vec<Child>,
    },
    Projection {
        parent: String,
        output: String,
    },
}

#[derive(Clone, Debug)]
pub struct Predicate {
    pub inputs: Vec<Port>,
    pub body: PredicateBody,
}

#[derive(Clone, Debug)]
pub enum PredicateBody {
    Declared {
        errors: Vec<Type>,
    },
    Defined {
        bindings: Vec<Binding>,
        condition: Condition,
    },
}

#[derive(Clone, Debug)]
pub enum Condition {
    Call {
        name: String,
        arguments: BTreeMap<String, Argument>,
    },
    All(Vec<Condition>),
    Any(Vec<Condition>),
    Not(Box<Condition>),
}

/// Bindings live only in this evaluation scope. Result text goes to the type's attribute parser.
#[derive(Clone, Debug)]
pub struct Block {
    pub bindings: Vec<Binding>,
    pub result: Argument,
    pub throwing: bool,
}

#[derive(Clone, Debug)]
pub struct Branch {
    pub condition: Option<Condition>,
    pub body: Block,
}

#[derive(Clone, Debug)]
pub struct Handler {
    pub reason: Option<Type>,
    pub action: HandlerAction,
}
#[derive(Clone, Debug)]
pub enum HandlerAction {
    Catch {
        name: Option<String>,
        branch: Branch,
    },
    Unreachable,
}
/// A child value, or a splice of the enclosing element's incoming sequence.
#[derive(Clone, Debug)]
pub enum Child {
    Value(Expression),
    Incoming { kind: Type },
}
#[derive(Clone, Debug)]
pub enum Argument {
    Missing {
        expected: TypeContract,
    },
    /// Exact XML Element reference, already checked against its receiving type.
    /// Native and interface attributes are interpreted through Attribute instead.
    Reference {
        name: String,
        expected: TypeContract,
    },
    /// Decoded attribute text, interpreted later by the receiving native type.
    Attribute {
        text: String,
        expected: TypeContract,
    },
}

impl Block {
    /// The protected result as written, for diagnostics on an evaluated try.
    pub fn result_text(&self) -> &str {
        match &self.result {
            Argument::Reference { name, .. } => name,
            Argument::Attribute { text, .. } => text,
            Argument::Missing { .. } => unreachable!("a block requires a result"),
        }
    }
}
