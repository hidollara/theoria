use super::{name::Identifier, parser::node::AstNode, types::Type};

/// `name` or `name: Type` in a lambda's parameter list.
///
/// Parameter names belong to the lambda syntax, not to the function type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parameter {
    pub name: AstNode<Identifier>,
    /// Only the annotation written in source. Whether an omitted type can be
    /// inferred from context is checked during semantic analysis.
    pub ty: Option<AstNode<Type>>,
}
