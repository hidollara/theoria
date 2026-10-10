//! Shared WGSL templates. Backends supply bindings and lower color transfers
//! before compiling the templates into their own pipelines.

pub const COMPOSITE: &str = include_str!("../shaders/composite.wgsl");
pub const MASK: &str = include_str!("../shaders/mask.wgsl");
pub const OUTPUT_IMAGE: &str = include_str!("../shaders/output_image.wgsl");
