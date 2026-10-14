//! Generate Rust construction code from Organon declarations.
//! Syntax, import resolution, and semantic validation belong to organon-core.
//! Native construction behavior belongs to organon-theoria.

mod api;
mod bind;
mod definition;
mod element;
mod instance;
mod ir;
mod types;

pub mod library;
mod native;
pub use organon_core::Error;
