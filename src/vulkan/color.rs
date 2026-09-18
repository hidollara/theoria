use crate::color::{Color, linear_to_srgb};

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

    pub(super) const fn from_color(color: Color) -> Self {
        let [red, green, blue, alpha] = color.to_straight_linear_srgba();
        Self([red * alpha, green * alpha, blue * alpha, alpha])
    }

    pub(super) const fn from_components(components: [f32; 4]) -> Self {
        Self(components)
    }

    pub(super) const fn components(self) -> [f32; 4] {
        self.0
    }

    pub(super) fn interpolate(self, other: Self, amount: f32) -> Self {
        Self(std::array::from_fn(|component| {
            self.0[component] + (other.0[component] - self.0[component]) * amount
        }))
    }

    pub(super) fn max_component_difference(self, other: Self) -> f32 {
        self.0
            .into_iter()
            .zip(other.0)
            .map(|(first, second)| (first - second).abs())
            .fold(0.0, f32::max)
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
            *component = (linear_to_srgb(linear) * 255.0).round() as u8;
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
    fn interpolation_operates_on_associated_components() {
        let transparent = PremultipliedLinearSrgba::TRANSPARENT;
        let opaque = PremultipliedLinearSrgba::from_color(Color::rgb8(255, 0, 0));

        assert_eq!(
            transparent.interpolate(opaque, 0.5).components(),
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
