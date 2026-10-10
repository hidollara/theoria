//! Font loading, text shaping and layout, and glyph realization as Canvas data.
//!
//! This crate does not require a window or GPU backend. SVG glyph realization
//! uses the renderer's backend-independent CPU color conversion support.

use theoria_canvas::{canvas, geometry, image, path, transform};
use theoria_color as color;

pub mod font;
pub mod text;
