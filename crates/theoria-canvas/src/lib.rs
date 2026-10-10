//! Declarative, backend-independent drawing data and shader operation definitions.

pub mod canvas;
pub mod image;

pub use theoria_geometry::{geometry, path, transform};

pub use canvas::*;
pub use theoria_color as color;
