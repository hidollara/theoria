//! Backend-internal Canvas evaluation helpers shared by physical renderers.

use crate::canvas::CanvasColor;

pub(crate) mod canvas_plan;
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

    fn interpolate(self, other: Self, amount: f32) -> Self {
        let first = self.components();
        let second = other.components();
        Self::from_components(std::array::from_fn(|component| {
            first[component] + (second[component] - first[component]) * amount
        }))
    }

    fn max_component_difference(self, other: Self) -> f32 {
        self.components()
            .into_iter()
            .zip(other.components())
            .map(|(first, second)| (first - second).abs())
            .fold(0.0, f32::max)
    }
}
