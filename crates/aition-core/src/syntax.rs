//! Data structures representing Aition source syntax.

/// Public syntax structures.
pub mod ast {
    pub use super::expr::Expr;
    pub use super::module::Module;
    pub use super::name::{Identifier, InvalidIdentifier, QualifiedName};
    pub use super::node::{AstNode, NodeSource, Trivia, TriviaKind};
    pub use super::types::Type;
}

mod expr;
mod module;
mod name;
mod node;
mod types;
