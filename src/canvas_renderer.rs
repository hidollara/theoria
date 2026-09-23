//! Backend-internal Canvas evaluation helpers shared by physical renderers.

use crate::canvas::CanvasColor;
use crate::color::RgbColorSpace;

pub(crate) mod canvas_plan;
pub(crate) mod color_space;
pub(crate) mod composition_encoding;
pub(crate) mod mesh_paint;
pub(crate) mod path_sampling;
pub(crate) mod view_space;

/// Color operations needed by CPU Canvas sampling without selecting a
/// renderer's physical working representation for it.
pub(crate) trait WorkingColor: Copy {
    const TRANSPARENT: Self;

    fn from_color(color: &CanvasColor) -> Self;
    fn from_components(components: [f32; 4]) -> Self;
    fn components(self) -> [f32; 4];

    fn interpolate(self, other: Self, amount: f32, space: RgbColorSpace) -> Self {
        Self::from_components(color_space::interpolate_premultiplied_linear_srgba(
            self.components(),
            other.components(),
            amount,
            space,
        ))
    }

    fn mix(colors: [Self; 3], weights: [f32; 3], space: RgbColorSpace) -> Self {
        Self::from_components(color_space::mix_premultiplied_linear_srgba(
            colors.map(Self::components),
            weights,
            space,
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
