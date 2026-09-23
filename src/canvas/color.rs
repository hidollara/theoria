use crate::color::{
    Color, ColorProfile, LabD50, Opacity, ReferenceSample, Rgb, RgbColorSpace, Sample, UnitF32,
    UnitF64, UnitSample, XyzD50, XyzD50Profile, XyzD65,
};

/// A logical color whose concrete representation has been erased at the
/// Canvas boundary without converting its coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CanvasColor {
    profile: CanvasProfile,
    coordinates: Coordinates,
    opacity: Alpha,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CanvasProfile {
    Rgb(RgbColorSpace),
    XyzD50,
    XyzD65,
    LabD50,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Coordinates {
    F32([f32; 3]),
    F64([f64; 3]),
    U8([u8; 3]),
    U16([u16; 3]),
}

impl Coordinates {
    fn as_f32(self) -> [f32; 3] {
        match self {
            Self::F32(coordinates) => coordinates,
            Self::F64(coordinates) => coordinates.map(|value| value as f32),
            Self::U8(coordinates) => coordinates.map(|value| f32::from(value) / f32::from(u8::MAX)),
            Self::U16(coordinates) => {
                coordinates.map(|value| f32::from(value) / f32::from(u16::MAX))
            }
        }
    }

    fn as_f64(self) -> [f64; 3] {
        match self {
            Self::F32(coordinates) => coordinates.map(Sample::to_f64),
            Self::F64(coordinates) => coordinates,
            Self::U8(coordinates) => coordinates.map(Sample::to_f64),
            Self::U16(coordinates) => coordinates.map(Sample::to_f64),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Alpha {
    F32(UnitF32),
    F64(UnitF64),
    U8(u8),
    U16(u16),
}

impl Alpha {
    fn as_f32(self) -> f32 {
        match self {
            Self::F32(opacity) => opacity.get(),
            Self::F64(opacity) => opacity.get() as f32,
            Self::U8(opacity) => f32::from(opacity) / f32::from(u8::MAX),
            Self::U16(opacity) => f32::from(opacity) / f32::from(u16::MAX),
        }
    }
}

impl CanvasColor {
    /// Converts authored coordinates only when a consumer selects its RGB
    /// evaluation space. Canvas retains the original profile and samples.
    pub(crate) fn straight_rgba_in(self, space: RgbColorSpace) -> [f32; 4] {
        let rgb = match self.profile {
            CanvasProfile::Rgb(authored_space) => {
                crate::color::convert_rgb(self.coordinates.as_f32(), authored_space, space)
            }
            CanvasProfile::XyzD50 => space
                .xyz_d50_to_coordinates(self.coordinates.as_f64())
                .map(|value| value as f32),
            CanvasProfile::XyzD65 => {
                let xyz = XyzD65.coordinates_to_xyz_d50(&self.coordinates.as_f64());
                space.xyz_d50_to_coordinates(xyz).map(|value| value as f32)
            }
            CanvasProfile::LabD50 => {
                let xyz = LabD50.coordinates_to_xyz_d50(&self.coordinates.as_f64());
                space.xyz_d50_to_coordinates(xyz).map(|value| value as f32)
            }
        };
        [rgb[0], rgb[1], rgb[2], self.opacity.as_f32()]
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
            profile: self.profile,
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

impl CanvasCoordinates for UnitF32 {
    fn erase(coordinates: [Self; 3]) -> Coordinates {
        Coordinates::F32(coordinates.map(UnitF32::get))
    }
}

impl CanvasCoordinates for f64 {
    fn erase(coordinates: [Self; 3]) -> Coordinates {
        Coordinates::F64(coordinates)
    }
}

impl CanvasCoordinates for UnitF64 {
    fn erase(coordinates: [Self; 3]) -> Coordinates {
        Coordinates::F64(coordinates.map(UnitF64::get))
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
            profile: CanvasProfile::Rgb(profile.into()),
            coordinates: S::erase(coordinates),
            opacity: A::erase(opacity),
        }
    }
}

macro_rules! canvas_reference_color {
    ($profile:ty, $variant:ident) => {
        impl<S, A> From<Color<$profile, S, A>> for CanvasColor
        where
            S: CanvasCoordinates + ReferenceSample<<$profile as ColorProfile>::Model>,
            A: CanvasOpacity,
        {
            fn from(color: Color<$profile, S, A>) -> Self {
                let (_, coordinates, opacity) = color.into_parts();
                Self {
                    profile: CanvasProfile::$variant,
                    coordinates: S::erase(coordinates),
                    opacity: A::erase(opacity),
                }
            }
        }
    };
}

canvas_reference_color!(XyzD50, XyzD50);
canvas_reference_color!(XyzD65, XyzD65);
canvas_reference_color!(LabD50, LabD50);

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
        assert_eq!(color.profile, CanvasProfile::Rgb(RgbColorSpace::Srgb));
        assert_eq!(
            color.straight_rgba_in(RgbColorSpace::Srgb),
            [1.0, 128.0 / 255.0, 64.0 / 255.0, 128.0 / 255.0]
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

        let canvas = CanvasColor::from(color);
        assert_eq!(canvas.profile, CanvasProfile::Rgb(RgbColorSpace::DisplayP3));
        assert_eq!(
            canvas.straight_rgba_in(RgbColorSpace::DisplayP3),
            [1.0, 0.5, 0.25, 0.75]
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
        assert_eq!(
            adjusted.profile,
            CanvasProfile::Rgb(RgbColorSpace::LinearDisplayP3)
        );
        assert!((adjusted.opacity.as_f32() - 64.0 / 255.0).abs() < 0.000_001);
    }

    #[test]
    fn converted_f64_color_enters_canvas_without_losing_its_storage() {
        let authored = Color::rgba(0.25, 0.5, 0.75, 0.4).unwrap();
        let converted = authored.convert_to(LinearSrgb).unwrap();
        let canvas = CanvasColor::from(converted);

        assert!(matches!(canvas.coordinates, Coordinates::F64(_)));
        assert!(matches!(canvas.opacity, Alpha::F32(_)));
        assert_eq!(
            canvas.profile,
            CanvasProfile::Rgb(RgbColorSpace::LinearSrgb)
        );
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

    #[test]
    fn xyz_and_lab_colors_remain_authored_until_renderer_lowering() {
        let source = Color::rgba(0.25, 0.5, 0.75, 0.4).unwrap();
        let original = CanvasColor::from(source);
        let lowered_original = crate::canvas_renderer::color_space::lower_straight_color(original);

        for (canvas, profile) in [
            (
                CanvasColor::from(source.convert_to(XyzD50).unwrap()),
                CanvasProfile::XyzD50,
            ),
            (
                CanvasColor::from(source.convert_to(XyzD65).unwrap()),
                CanvasProfile::XyzD65,
            ),
            (
                CanvasColor::from(source.convert_to(LabD50).unwrap()),
                CanvasProfile::LabD50,
            ),
        ] {
            assert_eq!(canvas.profile, profile);
            assert!(matches!(canvas.coordinates, Coordinates::F64(_)));
            assert!(matches!(canvas.opacity, Alpha::F32(_)));
            let lowered = crate::canvas_renderer::color_space::lower_straight_color(canvas);
            for (actual, expected) in lowered.into_iter().zip(lowered_original) {
                assert!((actual - expected).abs() < 0.000_01);
            }
        }
    }

    #[test]
    fn converted_lab_color_can_be_used_as_a_gradient_stop() {
        let source = Color::rgba(0.25, 0.5, 0.75, 0.4).unwrap();
        let stop =
            crate::canvas::GradientStop::new(0.5, source.convert_to(LabD50).unwrap()).unwrap();

        let rgba = stop.color().straight_rgba_in(RgbColorSpace::Srgb);
        for (actual, expected) in rgba.into_iter().zip([0.25, 0.5, 0.75, 0.4]) {
            assert!((actual - expected).abs() < 0.000_01);
        }
    }
}
