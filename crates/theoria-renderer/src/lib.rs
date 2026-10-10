//! Optional implementation building blocks for Canvas renderers.
//!
//! Backends choose these helpers; CPU tessellation and mesh painting are not
//! requirements imposed on every Canvas renderer.
//!
//! With no features this crate provides Canvas planning, color preparation,
//! raster upload preparation, and output descriptions. `wgsl` adds shader
//! lowering and templates, `tessellation` adds CPU path tessellation and
//! sampling, and `mesh-paint` adds CPU gradient subdivision on top of it.
//! GPU devices, textures, presentation, and command recording belong to the
//! backend crates rather than these shared helpers.

use theoria_canvas::canvas::Color;
#[cfg(feature = "wgsl")]
pub mod blend;
pub mod canvas_plan;
pub mod color_space;
pub mod composition;
#[cfg(feature = "wgsl")]
pub mod composition_encoding;
#[cfg(feature = "wgsl")]
pub mod composition_shader;
#[cfg(feature = "wgsl")]
pub mod icc_image_shader;
#[cfg(feature = "mesh-paint")]
pub mod mesh_paint;
mod output_image;
#[cfg(feature = "tessellation")]
pub mod path;
#[cfg(feature = "tessellation")]
pub mod path_sampling;
pub mod raster;
#[cfg(feature = "wgsl")]
pub mod rgb_shader;
#[cfg(feature = "wgsl")]
pub mod shaders;
#[cfg(feature = "tessellation")]
pub mod tessellation;
#[cfg(feature = "wgsl")]
pub mod transfer_wgsl;
#[cfg(feature = "tessellation")]
pub mod view_space;
mod viewport;

pub use output_image::{OutputAlpha, OutputImage};
pub use viewport::Viewport;

#[cfg(test)]
mod declaration_tests;
#[cfg(test)]
mod test_support;

/// Color operations needed by CPU Canvas sampling without selecting a
/// renderer's physical working representation for it.
pub trait WorkingColor: Copy {
    const TRANSPARENT: Self;

    fn from_color(color: &Color, prepared: &color_space::PreparedColors) -> Self;
    fn from_components(components: [f32; 4]) -> Self;
    fn components(self) -> [f32; 4];

    fn interpolate(
        self,
        other: Self,
        amount: f32,
        interpolation: &color_space::PreparedInterpolation,
    ) -> Self {
        Self::from_components(color_space::interpolate_premultiplied_rgba(
            self.components(),
            other.components(),
            amount,
            interpolation,
        ))
    }

    fn mix(
        colors: [Self; 3],
        weights: [f32; 3],
        interpolation: &color_space::PreparedInterpolation,
    ) -> Self {
        Self::from_components(color_space::mix_premultiplied_rgba(
            colors.map(Self::components),
            weights,
            interpolation,
        ))
    }

    fn max_component_difference(self, other: Self) -> f32 {
        self.components()
            .into_iter()
            .zip(other.components())
            .map(|(first, second)| (first - second).abs())
            .fold(0.0, f32::max)
    }
}
