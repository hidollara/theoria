use super::{
    body::Body, lambda::Lambda, literal::Literal, name::Identifier, parser::node::AstNode, tag::Tag,
};

/// An expression as written in source, before name resolution or type checking.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr {
    /// `"100"` or `"100"@Number`.
    Literal(AstNode<Literal>),
    /// `width`.
    Name(AstNode<Identifier>),
    /// `panels.Panel` or `makePanel(width).drawing`.
    ///
    /// Syntax does not distinguish module lookup from access to a value's member.
    Member {
        base: Box<AstNode<Expr>>,
        name: AstNode<Identifier>,
    },
    /// `makePanel(width)` or `makePainter(style)(panel)`.
    /// The callee is an expression, not necessarily a name.
    Call {
        callee: Box<AstNode<Expr>>,
        arguments: Vec<AstNode<Expr>>,
    },
    /// `<Panel width="100"/>` or `<Layer>(drawing)</Layer>`.
    Tag(AstNode<Tag>),
    /// `*(first, second)`, including the empty array `*()` and a trailing comma.
    Array(Vec<AstNode<Expr>>),
    /// `[width: Width] (width)` or `[width] (drawing) { drawing = makeDrawing(width); }`.
    Lambda(AstNode<Lambda>),
    /// `(expression)` with optional helper definitions.
    /// Parentheses belonging to a call are represented by `Call` instead.
    Body(AstNode<Body>),
}
