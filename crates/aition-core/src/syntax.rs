//! Data structures representing Aition source syntax.

/// Public syntax structures.
pub mod ast {
    pub use super::node::{AstNode, NodeSource, Trivia, TriviaKind};
}

mod node;
