use super::name::{Identifier, QualifiedName};
use super::types::Type;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr {
    Literal(Box<str>),
    Name(Identifier),
    Member {
        base: Box<Expr>,
        name: Identifier,
    },
    Call {
        callee: Box<Expr>,
        arguments: Vec<Expr>,
    },
    Ascribed {
        expr: Box<Expr>,
        ty: Type,
    },
    Array(Vec<Expr>),
    Tag(Tag),
    Body(Body),
    Lambda(Lambda),
    Match {
        arguments: Vec<Expr>,
        branches: Vec<Lambda>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Body {
    pub result: Box<Expr>,
    pub bindings: Option<Vec<(Identifier, Expr)>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lambda {
    pub parameters: Vec<Parameter>,
    pub body: Body,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parameter {
    pub name: Identifier,
    pub ty: Option<Type>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tag {
    pub name: QualifiedName,
    pub arguments: Vec<TagArgument>,
    /// A self-closing tag has no child list. Paired tags can have an empty list.
    pub children: Option<Vec<Expr>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagArgument {
    pub name: Identifier,
    /// A string literal or a parenthesized body.
    pub value: Expr,
}
