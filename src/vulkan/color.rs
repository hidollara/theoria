//! Conversion boundaries for the Vulkan Canvas color pipeline.
//!
//! Public colors and CPU images use straight alpha. Vulkan Canvas evaluation
//! lowers them into premultiplied linear sRGB exactly once, keeps that working
//! representation through interpolation and compositing, and converts it back
//! only when a CPU-readable image is requested.

use crate::canvas::CanvasColor;
use crate::canvas_renderer::{WorkingColor, color_space};

#[cfg(test)]
use crate::color::Color;

/// Vulkan Canvas working color in premultiplied linear-sRGB form.
///
/// This representation exists only after renderer lowering. Public colors
/// remain straight-alpha values, while every Vulkan Canvas pipeline consumes
/// this one compositing representation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PremultipliedLinearSrgba([f32; 4]);

/// Describes whether sampled RGB components already contain their alpha
/// contribution before they enter the premultiplied Canvas pipeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RgbAlphaAssociation {
    Straight,
    Premultiplied,
}

impl PremultipliedLinearSrgba {
    pub(super) const TRANSPARENT: Self = Self([0.0; 4]);

    pub(super) fn from_color(color: impl Into<CanvasColor>) -> Self {
        Self::from_canvas_color(color.into())
    }

    fn from_canvas_color(color: CanvasColor) -> Self {
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
        Self::from_canvas_color(*color)
    }

    fn from_components(components: [f32; 4]) -> Self {
        Self::from_components(components)
    }

    fn components(self) -> [f32; 4] {
        self.components()
    }
}

/// Converts quantized linear premultiplied RGBA readback into the public
/// straight-alpha sRGBA8 image representation.
pub(super) fn encode_linear_premultiplied_rgba8_as_straight_srgba8(pixels: &mut [u8]) {
    for pixel in pixels.chunks_exact_mut(4) {
        let alpha = f32::from(pixel[3]) / 255.0;
        if alpha == 0.0 {
            pixel[..3].fill(0);
            continue;
        }
        for component in &mut pixel[..3] {
            let linear = (f32::from(*component) / 255.0 / alpha).clamp(0.0, 1.0);
            *component = (color_space::encode_linear_srgb_component(linear) * 255.0).round() as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowering_associates_linear_rgb_with_alpha() {
        let lowered = PremultipliedLinearSrgba::from_color(Color::rgba8(255, 128, 64, 128));
        let [red, green, blue, alpha] = lowered.components();

        assert_eq!(alpha, 128.0 / 255.0);
        assert_eq!(red, alpha);
        assert!(green < red);
        assert!(blue < green);
    }

    #[test]
    fn lowering_decodes_srgb_into_the_vulkan_working_space() {
        let [red, green, blue, alpha] =
            PremultipliedLinearSrgba::from_color(Color::rgb(0.5, 0.5, 0.5).unwrap()).components();

        for component in [red, green, blue] {
            assert!((component - 0.214_041_14).abs() < 0.000_001);
        }
        assert_eq!(alpha, 1.0);
    }

    #[test]
    fn interpolation_operates_on_associated_components() {
        let transparent = PremultipliedLinearSrgba::TRANSPARENT;
        let opaque = PremultipliedLinearSrgba::from_color(Color::rgb8(255, 0, 0));

        assert_eq!(
            transparent
                .interpolate(opaque, 0.5, crate::color::LinearSrgb.into())
                .components(),
            [0.5, 0.0, 0.0, 0.5]
        );
    }

    #[test]
    fn readback_is_unassociated_and_encoded_as_srgb() {
        let mut pixels = [128, 0, 0, 128];

        encode_linear_premultiplied_rgba8_as_straight_srgba8(&mut pixels);

        assert_eq!(pixels, [255, 0, 0, 128]);
    }

    #[test]
    fn transparent_readback_discards_unobservable_rgb() {
        let mut pixels = [255, 128, 64, 0];

        encode_linear_premultiplied_rgba8_as_straight_srgba8(&mut pixels);

        assert_eq!(pixels, [0; 4]);
    }
}
