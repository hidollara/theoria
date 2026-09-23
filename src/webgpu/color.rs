use crate::{canvas::CanvasColor, canvas_renderer::WorkingColor};

#[cfg(test)]
use crate::color::Color;

/// WebGPU Canvas working color in premultiplied linear-sRGB form.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PremultipliedLinearSrgba([f32; 4]);

impl PremultipliedLinearSrgba {
    pub(super) const TRANSPARENT: Self = Self([0.0; 4]);

    pub(super) fn from_color(color: impl Into<CanvasColor>) -> Self {
        Self::from_straight_srgba(color.into().to_straight_srgba())
    }

    fn from_straight_srgba([red, green, blue, alpha]: [f32; 4]) -> Self {
        let [red, green, blue] = [red, green, blue].map(srgb_to_linear);
        Self([red * alpha, green * alpha, blue * alpha, alpha])
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
        Self::from_straight_srgba(color.to_straight_srgba())
    }

    fn from_components(components: [f32; 4]) -> Self {
        Self::from_components(components)
    }

    fn components(self) -> [f32; 4] {
        self.components()
    }
}

fn srgb_to_linear(component: f32) -> f32 {
    if component <= 0.04045 {
        component / 12.92
    } else {
        ((component + 0.055) / 1.055).powf(2.4)
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
