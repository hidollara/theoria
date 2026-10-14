//! Organon syntax, import resolution, and construction contracts, independent of execution backends.
mod error;
pub use error::Error;

pub mod identifier;
pub mod loader;
pub mod semantic;
pub mod syntax;
pub mod types;
#[cfg(feature = "xml")]
pub mod xml;
