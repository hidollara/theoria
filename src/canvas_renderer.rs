//! Backend-internal Canvas evaluation helpers shared by physical renderers.

use crate::canvas::CanvasColor;

pub(crate) mod canvas_plan;
pub(crate) mod color_space;
pub(crate) mod composition_encoding;
pub(crate) mod icc_image_shader;
pub(crate) mod mesh_paint;
pub(crate) mod output_color;
pub(crate) mod path_sampling;
pub(crate) mod raster;
pub(crate) mod view_space;

/// Color operations needed by CPU Canvas sampling without selecting a
/// renderer's physical working representation for it.
pub(crate) trait WorkingColor: Copy {
    const TRANSPARENT: Self;

    fn from_color(color: &CanvasColor, prepared: &color_space::PreparedColors) -> Self;
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
