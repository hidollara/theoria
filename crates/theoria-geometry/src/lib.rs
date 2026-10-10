//! Coordinate-system-neutral geometry, paths, and transformations.
//!
//! These descriptions are shared by drawing and input processing. They do not
//! depend on Canvas, color, rendering backends, or a window system.

pub mod geometry;
mod math;
pub mod path;
pub mod transform;
