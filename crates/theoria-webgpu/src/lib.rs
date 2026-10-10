//! WebGPU evaluation of backend-independent Canvas content.
//!
//! This crate deliberately stops at a caller-provided texture view. Window,
//! surface configuration, presentation, and event-loop integration remain
//! outside the Canvas renderer.
//!
//! The caller supplies a device, queue, and output texture. The renderer needs
//! no window handle or event loop, and can be used without the `theoria` crate.
//!
//! ```no_run
//! use theoria_canvas::{Canvas, CanvasView, Color, Opacity};
//! use theoria_color::Srgb;
//! use theoria_renderer::{SurfaceSize, Viewport};
//! use theoria_webgpu::{CanvasRenderer, RenderTarget, wgpu};
//!
//! fn draw(
//!     device: wgpu::Device,
//!     queue: wgpu::Queue,
//!     target: &wgpu::TextureView,
//!     size: SurfaceSize,
//!     canvas: &Canvas,
//!     view: CanvasView,
//! ) -> Result<(), Box<dyn std::error::Error>> {
//!     let mut renderer = CanvasRenderer::new(device, queue);
//!     let clear = Color::new(Srgb, [0_u8; 3], Opacity::<u8>::ONE);
//!     renderer.render(
//!         RenderTarget::new(target, wgpu::TextureFormat::Rgba8UnormSrgb, size, clear),
//!         Viewport::new(0.0, 0.0, size.width as f32, size.height as f32),
//!         canvas,
//!         view,
//!     )
//! }
//! ```

mod canvas_renderer;
mod color;

#[cfg(test)]
mod test_support;

pub use canvas_renderer::{CanvasRenderer, RenderTarget};
pub use wgpu;
