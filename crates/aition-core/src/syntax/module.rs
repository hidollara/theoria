use super::expr::{Expr, Parameter};
use super::name::{Identifier, QualifiedName};
use super::types::{Type, TypeParameter};

/// The contents of a module, with items in source order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Module {
    pub items: Vec<ModuleItem>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModuleItem {
    /// An inline module, such as `module controls { ... }`.
    Module { name: Identifier, body: Module },
    /// An alias, such as `alias Card = panels.Panel`.
    Alias {
        name: Identifier,
        target: QualifiedName,
    },
    /// A module-level binding declaration or definition.
    Binding { name: Identifier, body: BindingBody },
    /// A type declaration or definition.
    Type {
        name: Identifier,
        parameters: Option<Vec<TypeParameter>>,
        body: TypeBody,
    },
    /// A contract such as `contract eq<Rhs>: [Rhs](Bool)`.
    Contract {
        name: Identifier,
        parameters: Option<Vec<TypeParameter>>,
        result: Type,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeBody {
    Declaration {
        /// An explicit `[]` declares a zero-argument constructor.
        constructor: Option<Vec<Type>>,
        members: Vec<TypeMember>,
    },
    Definition {
        constructor: Vec<Parameter>,
        bindings: Vec<(Identifier, Expr)>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BindingBody {
    Declaration(Type),
    Definition(Expr),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeMember {
    pub name: Identifier,
    pub ty: Type,
}
