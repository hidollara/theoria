//! Geometry and delivery structures for interactive behavior.
//!
//! This layer is independent from Canvas drawing. A higher-level component can
//! derive matching visual and interactive structures without making either one
//! own the other.

mod hit_region;
mod tree;

pub use hit_region::{HitRegion, HitRegionError};
pub use tree::{Hit, InteractionTree};
