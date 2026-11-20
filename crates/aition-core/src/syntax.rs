//! Data structures representing Aition source syntax.

/// Public syntax structures.
pub mod ast {
    pub use super::module::{Module, ModuleItem, TypeMember};
    pub use super::name::{Identifier, InvalidIdentifier};
    pub use super::node::{AstNode, NodeSource, Trivia, TriviaKind};
}

mod module;
mod name;
mod node;
