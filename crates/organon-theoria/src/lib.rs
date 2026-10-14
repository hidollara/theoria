//! Connect this crate's Organon definitions to Rust with [`library!`].
//!
//! `library!(crate::module)` loads `organon.xml` beside Cargo.toml and generates
//! public types in its containing module. Rust callers use ordinary item paths;
//! XML imports connect to other crates without specifying their Rust module paths.
//! XML elements and Rust element implementations share the construction vocabulary here and
//! expose native Theoria outputs. There are no GUI dependencies.

// Typed construction and native contracts. IR execution belongs to the app.
mod contract;
mod elements;
mod source;

pub mod attribute;

pub use contract::{BuildError, Declaration, Evaluate, Implementation, Output, Provide, Type};
#[doc(hidden)]
pub use contract::{DefaultInterpret, DefaultParse};
pub use source::{Calculation, Read, ReadContext, Source};

#[doc(hidden)]
pub use source::BindingContext;
#[doc(hidden)]
pub use theoria_component as component;
#[doc(hidden)]
pub use theoria_reactive as reactive;

pub use organon_theoria_proc_macro::implement;
#[doc(hidden)]
pub use organon_theoria_proc_macro::library as __library;

/// Load the crate-root `organon.xml`, exposing its `export` definitions in this module.
/// Pass the containing module path, starting with `crate`. It is checked against
/// the actual placement. Call once per crate and use the generated items through
/// ordinary Rust paths. XML imports connect to this library automatically.
/// Also publishes `types` for public declarations and `attribute_types()` for attribute interpretation.
#[macro_export]
macro_rules! library {
    ($($input:tt)*) => {
        $crate::__library!($crate; $($input)*);
    };
}

library!(crate);
