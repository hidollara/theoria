use super::{
    expr::Expr,
    name::Identifier,
    node::AstNode,
    parser::{Parser, Syntax, choice, sequence, symbol},
    types::Type,
};
use crate::lexer::Symbol;

/// `name: Type;` or `name = expression;`.
///
/// This records the name and its declaration or definition in source syntax,
/// not its resolved binding identity.
/// Visibility and scope are determined by the containing syntax.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub name: AstNode<Identifier>,
    pub body: BindingBody,
}

/// The two grammatical forms of a binding's body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BindingBody {
    /// `: Type` declares the binding's type without providing an implementation.
    Declaration(AstNode<Type>),
    /// `= expression` provides a definition whose type is inferred.
    Definition(AstNode<Expr>),
}

impl Syntax for Binding {
    fn parser() -> impl Parser<Output = Self> {
        let declaration = sequence![symbol(Symbol::Colon), AstNode::<Type>::parser()]
            .map(|((), ty)| BindingBody::Declaration(ty));
        let definition = sequence![symbol(Symbol::Equal), AstNode::<Expr>::parser()]
            .map(|((), expr)| BindingBody::Definition(expr));
        sequence![
            AstNode::<Identifier>::parser(),
            choice![declaration, definition],
            symbol(Symbol::Semicolon),
        ]
        .map(|(name, body, ())| Self { name, body })
    }
}
