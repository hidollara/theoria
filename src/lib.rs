pub mod animation;
pub mod canvas;
pub mod color;
mod component_execution;
pub mod event_loop;
pub mod font;
pub mod geometry;
pub mod gesture;
pub mod image;
pub mod input;
pub mod interaction;
pub mod math;
pub mod path;
pub mod recording;
pub mod surface;
mod tessellation;
pub mod text;
pub mod transform;
pub mod vulkan;
pub mod window;

#[cfg(test)]
pub(crate) mod test_support;
pub mod windows;

pub use theoria_component as component;
pub use theoria_event as event;
pub use theoria_reactive as reactive;
