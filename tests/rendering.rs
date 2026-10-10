#[path = "rendering/color.rs"]
mod color;
#[path = "rendering/icc.rs"]
mod icc;
#[path = "rendering/image.rs"]
mod image;
#[path = "rendering/preparation.rs"]
mod preparation;
#[path = "rendering/reference.rs"]
mod reference;
#[cfg(any(feature = "vulkan", feature = "webgpu"))]
#[path = "rendering/wgsl.rs"]
mod wgsl;
