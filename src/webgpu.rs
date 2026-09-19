//! WebGPU evaluation of backend-independent Canvas content.
//!
//! This module deliberately stops at a caller-provided texture view. Window,
//! surface configuration, presentation, and event-loop integration remain
//! outside the Canvas renderer.

mod canvas_renderer;
mod color;

pub use canvas_renderer::{CanvasRenderer, RenderTarget};
pub use wgpu;
