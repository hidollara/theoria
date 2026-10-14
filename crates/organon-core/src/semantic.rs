//! Backend-independent construction IR and failure effect analysis.
//! Execution policy, native types and attribute parsers are supplied by a library.
pub mod effects;
mod ir;
mod types;
use crate::Error;
pub use ir::*;
pub use types::{ChildrenContract, Port, Type, TypeContract};
