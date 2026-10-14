//! Resolve declarations into a backend-independent construction graph.
//! Execution policy, native types and attribute parsers are supplied by a library.
mod ir;
mod resolve;
use crate::Error;
pub use ir::*;
pub use resolve::resolve;
