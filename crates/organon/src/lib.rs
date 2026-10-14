//! Import declarative Organon definitions into Rust with [`import!`].
//!
//! Place `organon::import!("src/panel.organon.xml");` inside a module to choose
//! its namespace. Paths are relative to the consuming package's Cargo.toml.
//! XML elements and Rust interfaces share the construction vocabulary here and
//! expose native Theoria outputs. There are no GUI dependencies.

mod implementation;
#[doc(hidden)]
pub use implementation::BindingContext;
pub use implementation::{
    Bound, BuildError, Calculation, Implementation, Interface, Read, ReadContext, Source, output,
};

pub use organon_proc_macro::import;
