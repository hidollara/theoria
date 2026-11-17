use super::{parser::node::AstNode, types::Type};

/// `"100"` or `"100"@Number`, before interpretation by a type's parser.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Literal {
    /// String contents after string escape decoding.
    pub text: Box<str>,
    /// Only an explicit `@Type`. A type required by the surrounding expression
    /// is not inserted here.
    pub ty: Option<AstNode<Type>>,
}
