//! Reusable geometry for interactive behavior.
//!
//! This layer is independent from Canvas drawing and event delivery. A
//! higher-level component can derive matching visual and interactive geometry
//! without making either one own the other.

mod hit_region;

pub use hit_region::{HitRegion, HitRegionError};
