//! Vulkan evaluation of backend-independent Canvas content.
//!
//! Offscreen image rendering has no window or event-loop dependency. The
//! optional `win32` feature adds swapchain presentation on Windows; the host
//! retains ownership of its native window and drives presentation explicitly.
//!
//! ```no_run
//! use theoria_canvas::{Canvas, CanvasView, Color, Coverage, Opacity, geometry::Point};
//! use theoria_color::Srgb;
//! use theoria_renderer::{SurfaceContent, SurfaceSize};
//! use theoria_vulkan::ImageSurface;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let color = Color::new(Srgb, [255_u8, 0, 0], Opacity::<u8>::ONE);
//! let canvas = Canvas::draw(color.clone(), Coverage::all());
//! let view = CanvasView::axis_aligned(Point::new(0.0, 0.0), Point::new(1.0, 1.0))
//!     .expect("non-degenerate view");
//! let mut surface = ImageSurface::new(SurfaceSize { width: 64, height: 64 })?;
//! let image = surface.render(&SurfaceContent::new(canvas, view, color))?;
//! assert_eq!(image.pixels().len(), 64 * 64 * 4);
//! # Ok(())
//! # }
//! ```

mod canvas_renderer;
mod color;
mod device;
mod host_buffer;
mod image;
mod image_layout;
mod image_surface;
mod one_shot;
#[cfg(all(feature = "win32", target_os = "windows"))]
mod presenter;
mod render_target;
mod scratch_image;

pub use canvas_renderer::CanvasRenderer;
pub use device::RenderDevice;
pub use image_surface::ImageSurface;
#[cfg(all(feature = "win32", target_os = "windows"))]
pub use presenter::{PresentFrame, PresentOutcome, Win32PresentationTarget, Win32Presenter};
pub use render_target::RenderTarget;

#[cfg(test)]
mod test_support;
