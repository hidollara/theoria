use super::name::Identifier;
use super::types::{Type, TypeParameter};

/// The contents of a module, with items in source order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Module {
    pub items: Vec<ModuleItem>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModuleItem {
    /// A module-level binding, such as `draw: [Panel](Drawing)`.
    Binding { name: Identifier, body: BindingBody },
    /// A type declaration such as `type Width: {}` or `type Br: [] {}`.
    Type {
        name: Identifier,
        parameters: Option<Vec<TypeParameter>>,
        /// An explicit `[]` declares a zero-argument constructor.
        constructor: Option<Vec<Type>>,
        members: Vec<TypeMember>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BindingBody {
    Declaration(Type),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeMember {
    pub name: Identifier,
    pub ty: Type,
}
