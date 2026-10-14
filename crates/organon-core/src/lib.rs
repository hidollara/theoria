//! Organon syntax, import resolution, and construction contracts, independent of execution backends.
mod error;
pub use error::Error;

pub mod format;
pub mod loader;
pub mod names;
pub mod semantic;
pub mod types;
