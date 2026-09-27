//! Conversion boundaries for the Vulkan Canvas color pipeline.
//!
//! Public colors and CPU images use straight alpha. Vulkan Canvas evaluation
//! lowers them into premultiplied renderer-selected RGB exactly once and keeps
//! that working representation through interpolation and compositing. The
//! destination image's color and alpha contract is applied at output.

use crate::canvas::CanvasColor;
use crate::canvas_renderer::{WorkingColor, color_space};

#[cfg(test)]
use crate::color::{Color, Opacity, Srgb};

/// Vulkan Canvas working color in premultiplied renderer-selected RGB coordinates.
///
/// This representation exists only after renderer lowering. Public colors
/// remain straight-alpha values, while every Vulkan Canvas pipeline consumes
/// this one compositing representation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PremultipliedWorkingRgba([f32; 4]);

/// Describes whether sampled RGB components already contain their alpha
/// contribution before they enter the premultiplied Canvas pipeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RgbAlphaAssociation {
    Straight,
    Premultiplied,
}

impl PremultipliedWorkingRgba {
    pub(super) const TRANSPARENT: Self = Self([0.0; 4]);

    #[cfg(test)]
    pub(super) fn from_color(color: impl Into<CanvasColor>) -> Self {
        Self::from_canvas_color(&color.into())
    }

    #[cfg(test)]
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

impl WorkingColor for PremultipliedWorkingRgba {
    const TRANSPARENT: Self = Self::TRANSPARENT;

    fn from_color(color: &CanvasColor, prepared: &color_space::PreparedColors) -> Self {
        Self(prepared.lower(color))
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
    fn lowering_associates_linear_rgb_with_alpha() {
        let lowered = PremultipliedWorkingRgba::from_color(Color::new(
            Srgb,
            [255_u8, 128, 64],
            Opacity::<u8>::from(128),
        ));
        let [red, green, blue, alpha] = lowered.components();

        assert_eq!(alpha, 128.0 / 255.0);
        assert_eq!(red, alpha);
        assert!(green < red);
        assert!(blue < green);
    }

    #[test]
    fn lowering_decodes_srgb_into_the_vulkan_working_space() {
        let [red, green, blue, alpha] = PremultipliedWorkingRgba::from_color(Color::new(
            Srgb,
            [0.5_f32, 0.5, 0.5],
            Opacity::<f32>::ONE,
        ))
        .components();

        for component in [red, green, blue] {
            assert!((component - 0.214_041_14).abs() < 0.000_001);
        }
        assert_eq!(alpha, 1.0);
    }

    #[test]
    fn interpolation_operates_on_associated_components() {
        let transparent = PremultipliedWorkingRgba::TRANSPARENT;
        let opaque = PremultipliedWorkingRgba::from_color(Color::new(
            Srgb,
            [255_u8, 0, 0],
            Opacity::<u8>::ONE,
        ));
        let mut prepared = crate::canvas_renderer::color_space::PreparedColors::default();
        prepared
            .prepare_interpolation(&crate::color::LinearSrgb.into())
            .unwrap();

        assert_eq!(
            transparent
                .interpolate(
                    opaque,
                    0.5,
                    prepared.interpolation(&crate::color::LinearSrgb.into()),
                )
                .components(),
            [0.5, 0.0, 0.0, 0.5]
        );
    }
}
