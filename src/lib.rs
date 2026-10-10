pub mod animation;
pub use theoria_canvas::{canvas, image, transform};
mod canvas_renderer;
pub use theoria_color as color;
mod component_execution;
pub mod event_loop;
pub mod font;
pub mod geometry;
pub mod gesture;
pub mod input;
pub mod interaction;
pub mod path;
pub mod recording;
pub mod surface;
pub mod text;

pub mod vulkan;
pub mod webgpu;
pub mod window;

#[cfg(test)]
pub(crate) mod test_support;
pub mod windows;

pub use theoria_component as component;
pub use theoria_event as event;
pub use theoria_reactive as reactive;

use theoria_renderer::{rgb_shader, tessellation};
