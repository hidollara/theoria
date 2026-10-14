//! Organon's graphical application library: editing data and interpreted previews.
//!
//! Editing uses the current authored Organon tree; previews execute its checked IR.
//!
//! XML persistence belongs to `organon-core`. Applications import drawings
//! through `organon_theoria::library!(crate::generated)` at compile time and use Theoria at runtime.
//! They do not depend on this application crate.

pub mod element;
pub mod evaluation;
pub mod preview;

#[doc(hidden)]
pub use organon_app_preview_macro::preview as __preview_library;
#[doc(hidden)]
pub use organon_theoria as runtime;

/// Compile native adapters into this preview host for the given XML entry point.
/// Publishes `registry()` in the invocation module. Providers remain ordinary libraries.
#[macro_export]
macro_rules! preview_library {
    ($path:literal) => {
        $crate::__preview_library!($crate; $path);
    };
}

pub use element::Element;

#[doc(hidden)]
pub mod generated {
    organon_theoria::library!(crate::generated);
}
