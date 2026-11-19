//! Type syntax and its precedence: grouping, array prefixes, then unions.

use super::{name::QualifiedName, node::AstNode};

/// A type expression represented by its grammatical structure.
/// Explicit grouping, alternative order, and repetitions are retained.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Type {
    /// `Panel` or `panels.Picture<Image>`.
    Named {
        name: AstNode<QualifiedName>,
        arguments: Vec<AstNode<Type>>,
    },
    /// `*T`; use `*(A | B)` for an array whose element type is a union.
    Array(Box<AstNode<Type>>),
    /// `[T, U](V)`; parameters contain type expressions.
    Function {
        parameters: Vec<AstNode<Type>>,
        result: Box<AstNode<Type>>,
    },
    /// `A | B | C`. Alternatives retain their order and any repetitions.
    Union(Vec<AstNode<Type>>),
    /// `"(" Type ")"`. Each explicit pair of grouping parentheses is retained.
    Group(Box<AstNode<Type>>),
}
