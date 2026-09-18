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

    pub(super) const fn from_color(color: Color) -> Self {
        let [red, green, blue, alpha] = color.to_linear_srgba();
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
}
