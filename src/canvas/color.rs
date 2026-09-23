use crate::color::{Color, DisplayP3, LinearDisplayP3, LinearSrgb, RgbColorSpace, Srgb, UnitF32};

/// A logical color whose concrete representation has been erased at the
/// Canvas boundary without converting its coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CanvasColor {
    color_space: RgbColorSpace,
    coordinates: Coordinates,
    opacity: Alpha,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Coordinates {
    F32([f32; 3]),
    U8([u8; 3]),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Alpha {
    F32(UnitF32),
    U8(u8),
}

impl CanvasColor {
    /// Returns normalized straight-alpha coordinates in their authored RGB
    /// space. Renderer lowering decides how and at what precision to convert
    /// them into its working representation.
    pub(crate) fn authored_straight_rgba(self) -> ([f32; 4], RgbColorSpace) {
        const BYTE_TO_UNIT: f32 = 1.0 / 255.0;
        let [red, green, blue] = match self.coordinates {
            Coordinates::F32(coordinates) => coordinates,
            Coordinates::U8(coordinates) => {
                coordinates.map(|value| f32::from(value) * BYTE_TO_UNIT)
            }
        };
        let alpha = match self.opacity {
            Alpha::F32(opacity) => opacity.get(),
            Alpha::U8(opacity) => f32::from(opacity) * BYTE_TO_UNIT,
        };
        ([red, green, blue, alpha], self.color_space)
    }

    pub(crate) fn straight_rgba_in(self, space: RgbColorSpace) -> [f32; 4] {
        let ([red, green, blue, alpha], authored_space) = self.authored_straight_rgba();
        let [red, green, blue] =
            crate::color::convert_rgb([red, green, blue], authored_space, space);
        [red, green, blue, alpha]
    }

    pub(crate) fn with_alpha_factor(&self, factor: f32) -> Self {
        let alpha = match self.opacity {
            Alpha::F32(opacity) => opacity.get(),
            Alpha::U8(opacity) => f32::from(opacity) / 255.0,
        };
        Self {
            color_space: self.color_space,
            coordinates: self.coordinates,
            opacity: Alpha::F32(
                UnitF32::new(alpha * factor.clamp(0.0, 1.0))
                    .expect("a finite unit opacity remains valid after multiplication"),
            ),
        }
    }
}

macro_rules! canvas_rgb_color {
    ($profile:ty, $space:expr) => {
        impl From<Color<$profile, f32, UnitF32>> for CanvasColor {
            fn from(color: Color<$profile, f32, UnitF32>) -> Self {
                let (_, coordinates, opacity) = color.into_parts();
                Self {
                    color_space: $space,
                    coordinates: Coordinates::F32(coordinates),
                    opacity: Alpha::F32(opacity.sample()),
                }
            }
        }

        impl From<Color<$profile, u8, u8>> for CanvasColor {
            fn from(color: Color<$profile, u8, u8>) -> Self {
                let (_, coordinates, opacity) = color.into_parts();
                Self {
                    color_space: $space,
                    coordinates: Coordinates::U8(coordinates),
                    opacity: Alpha::U8(opacity.sample()),
                }
            }
        }
    };
}

canvas_rgb_color!(Srgb, RgbColorSpace::Srgb);
canvas_rgb_color!(LinearSrgb, RgbColorSpace::LinearSrgb);
canvas_rgb_color!(DisplayP3, RgbColorSpace::DisplayP3);
canvas_rgb_color!(LinearDisplayP3, RgbColorSpace::LinearDisplayP3);

impl From<&CanvasColor> for CanvasColor {
    fn from(color: &CanvasColor) -> Self {
        *color
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn erasing_a_color_does_not_normalize_its_storage() {
        let color = CanvasColor::from(Color::rgba8(255, 128, 64, 128));

        assert!(matches!(color.coordinates, Coordinates::U8(_)));
        assert_eq!(
            color.authored_straight_rgba(),
            (
                [1.0, 128.0 / 255.0, 64.0 / 255.0, 128.0 / 255.0],
                RgbColorSpace::Srgb
            )
        );
    }

    #[test]
    fn erasing_a_color_retains_its_rgb_profile() {
        let color = Color::new(
            DisplayP3,
            [UnitF32::ONE.get(), 0.5, 0.25],
            crate::color::Opacity::new(UnitF32::new(0.75).unwrap()),
        )
        .unwrap();

        assert_eq!(
            CanvasColor::from(color).authored_straight_rgba(),
            ([1.0, 0.5, 0.25, 0.75], RgbColorSpace::DisplayP3)
        );
    }

    #[test]
    fn changing_opacity_preserves_authored_coordinates_and_profile() {
        let color = CanvasColor::from(
            Color::new(
                LinearDisplayP3,
                [255_u8, 128, 64],
                crate::color::Opacity::new(128_u8),
            )
            .unwrap(),
        );

        let adjusted = color.with_alpha_factor(0.5);

        assert_eq!(adjusted.coordinates, Coordinates::U8([255, 128, 64]));
        assert_eq!(adjusted.color_space, RgbColorSpace::LinearDisplayP3);
        assert!((adjusted.authored_straight_rgba().0[3] - 64.0 / 255.0).abs() < 0.000_001);
    }
}
