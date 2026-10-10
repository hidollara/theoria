//! Declarative Canvas, Components, and platform integration.
//!
//! The default `vulkan` and `webgpu` features re-export their standalone rendering
//! backends. The concrete Win32 Window frontend currently requires `vulkan`;
//! the Win32 event loop and message types remain available without either backend.

pub mod animation;
pub use theoria_canvas::{canvas, image, transform};
pub use theoria_color as color;
#[cfg(any(feature = "vulkan", test))]
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

#[cfg(feature = "vulkan")]
pub use theoria_vulkan as vulkan;
#[cfg(feature = "webgpu")]
pub use theoria_webgpu as webgpu;
pub mod window;

#[cfg(test)]
pub(crate) mod test_support;
pub mod windows;

pub use theoria_component as component;
pub use theoria_event as event;
pub use theoria_reactive as reactive;
