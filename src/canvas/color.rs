use crate::color::{
    Color, ColorProfile, Opacity, Rgb, RgbColorSpace, Sample, UnitF32, UnitF64, UnitSample,
};

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
    F64([f64; 3]),
    U8([u8; 3]),
    U16([u16; 3]),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Alpha {
    F32(UnitF32),
    F64(UnitF64),
    U8(u8),
    U16(u16),
}

impl CanvasColor {
    /// Returns normalized straight-alpha coordinates in their authored RGB
    /// space. Renderer lowering decides how and at what precision to convert
    /// them into its working representation.
    pub(crate) fn authored_straight_rgba(self) -> ([f32; 4], RgbColorSpace) {
        const BYTE_TO_UNIT: f32 = 1.0 / 255.0;
        let [red, green, blue] = match self.coordinates {
            Coordinates::F32(coordinates) => coordinates,
            Coordinates::F64(coordinates) => coordinates.map(|value| value as f32),
            Coordinates::U8(coordinates) => {
                coordinates.map(|value| f32::from(value) * BYTE_TO_UNIT)
            }
            Coordinates::U16(coordinates) => {
                coordinates.map(|value| f32::from(value) / f32::from(u16::MAX))
            }
        };
        let alpha = match self.opacity {
            Alpha::F32(opacity) => opacity.get(),
            Alpha::F64(opacity) => opacity.get() as f32,
            Alpha::U8(opacity) => f32::from(opacity) * BYTE_TO_UNIT,
            Alpha::U16(opacity) => f32::from(opacity) / f32::from(u16::MAX),
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
        let factor = factor.clamp(0.0, 1.0);
        let opacity = match self.opacity {
            Alpha::F32(opacity) => Alpha::F32(
                UnitF32::new(opacity.get() * factor)
                    .expect("a finite unit opacity remains valid after multiplication"),
            ),
            Alpha::F64(opacity) => Alpha::F64(
                UnitF64::new(opacity.get() * f64::from(factor))
                    .expect("a finite unit opacity remains valid after multiplication"),
            ),
            Alpha::U8(opacity) => Alpha::F32(
                UnitF32::new(f32::from(opacity) / 255.0 * factor)
                    .expect("a finite unit opacity remains valid after multiplication"),
            ),
            Alpha::U16(opacity) => Alpha::F32(
                UnitF32::new(f32::from(opacity) / f32::from(u16::MAX) * factor)
                    .expect("a finite unit opacity remains valid after multiplication"),
            ),
        };
        Self {
            color_space: self.color_space,
            coordinates: self.coordinates,
            opacity,
        }
    }
}

trait CanvasCoordinates: Sample {
    fn erase(coordinates: [Self; 3]) -> Coordinates;
}

impl CanvasCoordinates for f32 {
    fn erase(coordinates: [Self; 3]) -> Coordinates {
        Coordinates::F32(coordinates)
    }
}

impl CanvasCoordinates for f64 {
    fn erase(coordinates: [Self; 3]) -> Coordinates {
        Coordinates::F64(coordinates)
    }
}

impl CanvasCoordinates for u8 {
    fn erase(coordinates: [Self; 3]) -> Coordinates {
        Coordinates::U8(coordinates)
    }
}

impl CanvasCoordinates for u16 {
    fn erase(coordinates: [Self; 3]) -> Coordinates {
        Coordinates::U16(coordinates)
    }
}

trait CanvasOpacity: UnitSample {
    fn erase(opacity: Opacity<Self>) -> Alpha;
}

impl CanvasOpacity for UnitF32 {
    fn erase(opacity: Opacity<Self>) -> Alpha {
        Alpha::F32(opacity.sample())
    }
}

impl CanvasOpacity for UnitF64 {
    fn erase(opacity: Opacity<Self>) -> Alpha {
        Alpha::F64(opacity.sample())
    }
}

impl CanvasOpacity for u8 {
    fn erase(opacity: Opacity<Self>) -> Alpha {
        Alpha::U8(opacity.sample())
    }
}

impl CanvasOpacity for u16 {
    fn erase(opacity: Opacity<Self>) -> Alpha {
        Alpha::U16(opacity.sample())
    }
}

impl<P, S, A> From<Color<P, S, A>> for CanvasColor
where
    P: ColorProfile<Model = Rgb> + Into<RgbColorSpace>,
    S: CanvasCoordinates,
    A: CanvasOpacity,
{
    fn from(color: Color<P, S, A>) -> Self {
        let (profile, coordinates, opacity) = color.into_parts();
        Self {
            color_space: profile.into(),
            coordinates: S::erase(coordinates),
            opacity: A::erase(opacity),
        }
    }
}

impl From<&CanvasColor> for CanvasColor {
    fn from(color: &CanvasColor) -> Self {
        *color
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::{DisplayP3, LinearDisplayP3, LinearSrgb, Srgb};

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

    #[test]
    fn converted_f64_color_enters_canvas_without_losing_its_storage() {
        let authored = Color::rgba(0.25, 0.5, 0.75, 0.4).unwrap();
        let converted = authored.convert_to(LinearSrgb).unwrap();
        let canvas = CanvasColor::from(converted);

        assert!(matches!(canvas.coordinates, Coordinates::F64(_)));
        assert!(matches!(canvas.opacity, Alpha::F32(_)));
        assert_eq!(canvas.color_space, RgbColorSpace::LinearSrgb);
        let rgba = canvas.straight_rgba_in(RgbColorSpace::Srgb);
        for (actual, expected) in rgba.into_iter().zip([0.25, 0.5, 0.75, 0.4]) {
            assert!((actual - expected).abs() < 0.000_01);
        }
    }

    #[test]
    fn high_precision_and_integer_samples_are_erased_independently() {
        let color = Color::new(
            DisplayP3,
            [0.125_f64, 0.25, 0.5],
            Opacity::new(UnitF64::new(0.75).unwrap()),
        )
        .unwrap();
        let canvas = CanvasColor::from(color);
        assert!(matches!(canvas.coordinates, Coordinates::F64(_)));
        assert!(matches!(canvas.opacity, Alpha::F64(_)));
        assert!(matches!(
            canvas.with_alpha_factor(0.5).opacity,
            Alpha::F64(_)
        ));

        let color =
            Color::new(Srgb, [0_u16, u16::MAX / 2, u16::MAX], Opacity::new(128_u8)).unwrap();
        let canvas = CanvasColor::from(color);
        assert!(matches!(canvas.coordinates, Coordinates::U16(_)));
        assert!(matches!(canvas.opacity, Alpha::U8(_)));
    }
}
