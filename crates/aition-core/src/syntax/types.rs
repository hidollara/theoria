use super::name::{Identifier, QualifiedName};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Type {
    Named(NamedType),
    /// An array type, written `*T`.
    Array(Box<Type>),
    /// An explicitly parenthesized type.
    Group(Box<Type>),
    /// A function type, written `[A, B](Result)`.
    Function {
        parameters: Vec<Type>,
        result: Box<Type>,
    },
    /// Alternatives in source order, including repetitions.
    Union(Vec<Type>),
}

/// A qualified name with optional type arguments, such as `ui.Picture<Image>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamedType {
    pub name: QualifiedName,
    pub arguments: Option<Vec<Type>>,
}

/// A type parameter with its contract bounds, such as `T: drawingOf & eq<U>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeParameter {
    pub name: Identifier,
    pub bounds: Vec<ContractRef>,
}

/// A reference to a contract, such as `ui.drawingOf` or `eq<U>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContractRef {
    pub name: QualifiedName,
    pub arguments: Option<Vec<Type>>,
}
