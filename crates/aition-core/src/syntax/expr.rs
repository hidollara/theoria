use super::{body::Body, lambda::Lambda, name::Identifier, parser::node::AstNode, types::Type};

/// An expression as written in source, before name resolution or type checking.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr {
    /// `"100"` or `"100"@Number`.
    ///
    /// `text` contains the string contents after string escape decoding, before
    /// interpretation by a type's parser. `ty` records only an explicit `@Type`;
    /// a type required by the surrounding expression is not inserted here.
    Literal {
        text: String,
        ty: Option<AstNode<Type>>,
    },
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
    /// `*(first, second)`, including the empty array `*()` and a trailing comma.
    Array(Vec<AstNode<Expr>>),
    /// `[width: Width] (width)` or `[width] (drawing) { drawing = makeDrawing(width); }`.
    Lambda(AstNode<Lambda>),
    /// `(expression)` with optional helper definitions.
    /// Parentheses belonging to a call are represented by `Call` instead.
    Body(AstNode<Body>),
}
