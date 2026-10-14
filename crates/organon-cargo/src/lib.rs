//! Connect Organon definitions to Cargo packages and Rust library providers.
//! Execution-backend code generation belongs to the corresponding compiler.
mod library;
mod link;
mod paths;
mod sources;

pub use library::{LoadedLibrary, declaration_path, identity, load, manifest};
pub use link::{connect as connect_library_paths, export_connector, track_files};
pub use organon_core::Error;
pub use paths::replace_crate_path;
pub use sources::{CargoSources, source_path};
