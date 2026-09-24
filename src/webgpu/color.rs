use crate::{
    canvas::CanvasColor,
    canvas_renderer::{WorkingColor, color_space},
};

#[cfg(test)]
use crate::color::Color;

/// WebGPU Canvas working color in premultiplied linear-sRGB form.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PremultipliedLinearSrgba([f32; 4]);

impl PremultipliedLinearSrgba {
    pub(super) const TRANSPARENT: Self = Self([0.0; 4]);

    pub(super) fn from_color(color: impl Into<CanvasColor>) -> Self {
        Self::from_canvas_color(&color.into())
    }

    fn from_canvas_color(color: &CanvasColor) -> Self {
        Self(color_space::lower_straight_color(color))
    }

    pub(super) const fn from_components(components: [f32; 4]) -> Self {
        Self(components)
    }

    pub(super) const fn components(self) -> [f32; 4] {
        self.0
    }
}

impl WorkingColor for PremultipliedLinearSrgba {
    const TRANSPARENT: Self = Self::TRANSPARENT;

    fn from_color(color: &CanvasColor) -> Self {
        Self::from_canvas_color(color)
    }

    fn from_components(components: [f32; 4]) -> Self {
        Self::from_components(components)
    }

    fn components(self) -> [f32; 4] {
        self.components()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowering_decodes_and_associates_public_colors() {
        let [red, green, blue, alpha] =
            PremultipliedLinearSrgba::from_color(Color::rgba(0.5, 0.5, 0.5, 0.5).unwrap())
                .components();

        for component in [red, green, blue] {
            assert!((component - 0.107_020_57).abs() < 0.000_001);
        }
        assert_eq!(alpha, 0.5);
    }
}
