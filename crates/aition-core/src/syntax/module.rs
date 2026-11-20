use super::name::Identifier;
use super::node::AstNode;

/// The contents of a module, with items in source order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Module {
    pub items: Vec<AstNode<ModuleItem>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModuleItem {
    /// A type declaration such as `type Width: {}` or `type Br: [] {}`.
    Type {
        name: AstNode<Identifier>,
        /// An explicit `[]` declares a zero-argument constructor.
        constructor: Option<AstNode<Vec<AstNode<Identifier>>>>,
        members: AstNode<Vec<AstNode<TypeMember>>>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeMember {
    pub name: AstNode<Identifier>,
    pub ty: AstNode<Identifier>,
}
