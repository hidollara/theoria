//! Organon's graphical application library: editing data and interpreted previews.
//!
//! An [`Element`] declares inputs and named outputs. A [`CanvasExpr`] describes
//! how to construct one Canvas value, which can be explicitly exported.
//!
//! XML persistence belongs to `organon-format`. Applications import drawings
//! through `organon::import!` at compile time and use Theoria at runtime.
//! They do not depend on this application crate.

pub mod element;
pub mod evaluation;
pub mod preview;

pub use element::{CanvasExpr, Element};
