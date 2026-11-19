use super::{
    name::Identifier,
    node::AstNode,
    parser::{Parser, Syntax, optional, sequence, symbol},
    types::Type,
};
use crate::lexer::Symbol;

/// `name` or `name: Type` in a lambda's parameter list.
/// Names belong to the lambda syntax, not to the function type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parameter {
    pub name: AstNode<Identifier>,
    /// Only an explicit annotation; inference belongs to semantic analysis.
    pub ty: Option<AstNode<Type>>,
}

impl Syntax for Parameter {
    fn parser() -> impl Parser<Output = Self> {
        sequence![
            AstNode::<Identifier>::parser(),
            optional(
                sequence![symbol(Symbol::Colon), AstNode::<Type>::parser()].map(|((), ty)| ty)
            ),
        ]
        .map(|(name, ty)| Self { name, ty })
    }
}
