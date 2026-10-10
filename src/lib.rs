//! Declarative Canvas, Components, and platform integration.
//!
//! The default `vulkan` and `webgpu` features re-export their standalone rendering
//! backends. Win32 Windows, input, and the event loop remain available without
//! either backend; Vulkan adds the Canvas presentation constructors.

pub use theoria_canvas::{canvas, image, transform};
pub use theoria_color as color;
pub use theoria_runtime::{animation, event_loop, input, surface, window};
pub mod geometry;
pub mod gesture;
pub mod interaction;
pub mod path;
pub mod recording;
pub use theoria_text::{font, text};

#[cfg(feature = "vulkan")]
pub use theoria_vulkan as vulkan;
#[cfg(feature = "webgpu")]
pub use theoria_webgpu as webgpu;

#[cfg(test)]
pub(crate) mod test_support;
pub mod windows;

pub use theoria_component as component;
pub use theoria_event as event;
pub use theoria_reactive as reactive;
