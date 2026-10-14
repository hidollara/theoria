//! Organon syntax, import resolution, and construction contracts, independent of execution backends.
mod error;
pub use error::Error;

pub mod resolver;
pub mod semantic;
pub mod syntax;
#[cfg(feature = "xml")]
pub mod xml;
