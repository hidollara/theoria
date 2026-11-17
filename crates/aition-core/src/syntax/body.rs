use super::{binding::Binding, expr::Expr, parser::node::AstNode};

/// A result expression and its optional helper definitions:
/// `(expression)` or `(expression) { name = other; ... }`.
///
/// Each explicit pair of expression-grouping parentheses is retained, even
/// when no helper definitions are present. Nested pairs contain nested `Expr::Body`s.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Body {
    /// The result expression, which can refer to the helper bindings and
    /// names from the enclosing context.
    pub result: Box<AstNode<Expr>>,
    /// Local bindings, not statements executed after evaluating the result.
    /// Source order is retained without prescribing evaluation order.
    /// `None` means no braces were written; `Some` retains even an empty `{}`.
    pub bindings: Option<Vec<AstNode<Binding>>>,
}
