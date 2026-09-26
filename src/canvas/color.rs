use std::{error::Error, fmt, sync::Arc};

use crate::color::{
    Color, ColorProfile, ColorTransformError, IccProfile, IccProfileError, IccRenderingIntent,
    LabD50, Opacity, ReferenceSample, RgbColorSpace, Sample, TransferError, UnitF32, UnitF64,
    UnitSample, XyzD50, XyzD65,
};

/// A logical color whose concrete representation has been erased at the
/// Canvas boundary without converting its coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct CanvasColor {
    pub(crate) profile: CanvasProfile,
    pub(crate) coordinates: Coordinates,
    pub(crate) opacity: Alpha,
}

#[derive(Clone, Debug)]
pub(crate) enum CanvasProfile {
    Rgb(RgbColorSpace),
    XyzD50,
    XyzD65,
    LabD50,
    Icc {
        profile: Arc<IccProfile>,
        intent: IccRenderingIntent,
    },
}

impl PartialEq for CanvasProfile {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Rgb(left), Self::Rgb(right)) => left == right,
            (Self::XyzD50, Self::XyzD50)
            | (Self::XyzD65, Self::XyzD65)
            | (Self::LabD50, Self::LabD50) => true,
            (
                Self::Icc {
                    profile: left,
                    intent: left_intent,
                    ..
                },
                Self::Icc {
                    profile: right,
                    intent: right_intent,
                    ..
                },
            ) => left.device_space() == right.device_space() && left_intent == right_intent,
            _ => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Coordinates {
    F32([f32; 3]),
    F64([f64; 3]),
    U8([u8; 3]),
    U16([u16; 3]),
    Dynamic(Arc<[f64]>),
}

impl Coordinates {
    pub(crate) fn as_f32(&self) -> [f32; 3] {
        match self {
            Self::F32(coordinates) => *coordinates,
            Self::F64(coordinates) => coordinates.map(|value| value as f32),
            Self::U8(coordinates) => coordinates.map(|value| f32::from(value) / f32::from(u8::MAX)),
            Self::U16(coordinates) => {
                coordinates.map(|value| f32::from(value) / f32::from(u16::MAX))
            }
            Self::Dynamic(_) => unreachable!("ICC coordinates are evaluated by their profile"),
        }
    }

    pub(crate) fn as_f64(&self) -> [f64; 3] {
        match self {
            Self::F32(coordinates) => coordinates.map(Sample::to_f64),
            Self::F64(coordinates) => *coordinates,
            Self::U8(coordinates) => coordinates.map(Sample::to_f64),
            Self::U16(coordinates) => coordinates.map(Sample::to_f64),
            Self::Dynamic(_) => unreachable!("ICC coordinates are evaluated by their profile"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Alpha {
    F32(UnitF32),
    F64(UnitF64),
    U8(u8),
    U16(u16),
}

impl Alpha {
    pub(crate) fn as_f32(self) -> f32 {
        match self {
            Self::F32(opacity) => opacity.get(),
            Self::F64(opacity) => opacity.get() as f32,
            Self::U8(opacity) => f32::from(opacity) / f32::from(u8::MAX),
            Self::U16(opacity) => f32::from(opacity) / f32::from(u16::MAX),
        }
    }
}

impl CanvasColor {
    /// Keeps authored ICC device coordinates, profile, and rendering intent.
    /// The renderer selects how to convert them into its working space.
    pub fn from_icc(
        profile: &IccProfile,
        coordinates: impl Into<Arc<[f64]>>,
        opacity: UnitF32,
        intent: IccRenderingIntent,
    ) -> Result<Self, CanvasIccColorError> {
        if intent == IccRenderingIntent::AbsoluteColorimetric {
            return Err(CanvasIccColorError::AbsoluteIntent);
        }
        let coordinates = coordinates.into();
        if coordinates.len() != profile.device_channels()
            || coordinates.iter().any(|value| !value.is_finite())
        {
            return Err(CanvasIccColorError::InvalidCoordinates);
        }
        Ok(Self {
            profile: CanvasProfile::Icc {
                profile: Arc::new(profile.clone()),
                intent,
            },
            coordinates: Coordinates::Dynamic(coordinates),
            opacity: Alpha::F32(opacity),
        })
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
            profile: self.profile.clone(),
            coordinates: self.coordinates.clone(),
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

macro_rules! canvas_rgb_color {
    ($profile:ty) => {
        impl<S, A> From<Color<$profile, S, A>> for CanvasColor
        where
            S: CanvasCoordinates,
            A: CanvasOpacity,
        {
            fn from(color: Color<$profile, S, A>) -> Self {
                let (profile, coordinates, opacity) = color.into_parts();
                Self {
                    profile: CanvasProfile::Rgb(profile.into()),
                    coordinates: S::erase(coordinates),
                    opacity: A::erase(opacity),
                }
            }
        }
    };
}

canvas_rgb_color!(crate::color::Srgb);
canvas_rgb_color!(crate::color::LinearSrgb);
canvas_rgb_color!(crate::color::DisplayP3);
canvas_rgb_color!(crate::color::LinearDisplayP3);
canvas_rgb_color!(RgbColorSpace);

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
        color.clone()
    }
}

#[derive(Debug)]
pub enum CanvasIccColorError {
    AbsoluteIntent,
    InvalidCoordinates,
    Profile(IccProfileError),
    Transform(ColorTransformError),
    Transfer(TransferError),
}

impl From<ColorTransformError> for CanvasIccColorError {
    fn from(error: ColorTransformError) -> Self {
        Self::Transform(error)
    }
}

impl From<IccProfileError> for CanvasIccColorError {
    fn from(error: IccProfileError) -> Self {
        Self::Profile(error)
    }
}

impl From<TransferError> for CanvasIccColorError {
    fn from(error: TransferError) -> Self {
        Self::Transfer(error)
    }
}

impl fmt::Display for CanvasIccColorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AbsoluteIntent => {
                formatter.write_str("absolute ICC color needs an explicit display conversion")
            }
            Self::InvalidCoordinates => formatter.write_str("ICC device coordinates are invalid"),
            Self::Profile(error) => error.fmt(formatter),
            Self::Transform(error) => error.fmt(formatter),
            Self::Transfer(error) => error.fmt(formatter),
        }
    }
}

impl Error for CanvasIccColorError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Profile(error) => Some(error),
            Self::Transform(error) => Some(error),
            Self::Transfer(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::{DisplayP3, LinearDisplayP3, LinearSrgb, Srgb};

    #[test]
    fn icc_color_preserves_device_coordinates_until_renderer_lowering() {
        let bytes = moxcms::ColorProfile::new_srgb().encode().unwrap();
        let profile = IccProfile::from_bytes(&bytes).unwrap();
        let coordinates: Arc<[f64]> = vec![0.25, 0.5, 0.75].into();
        let color = CanvasColor::from_icc(
            &profile,
            coordinates.clone(),
            UnitF32::new(0.5).unwrap(),
            IccRenderingIntent::RelativeColorimetric,
        )
        .unwrap();
        let Coordinates::Dynamic(stored) = &color.coordinates else {
            panic!("ICC device coordinates were converted at the Canvas boundary")
        };
        assert!(Arc::ptr_eq(stored, &coordinates));
        let CanvasProfile::Icc {
            profile: stored_profile,
            intent,
            ..
        } = &color.profile
        else {
            panic!("Canvas must retain the declared ICC profile, not a conversion plan")
        };
        assert_eq!(stored_profile.device_space(), profile.device_space());
        assert_eq!(*intent, IccRenderingIntent::RelativeColorimetric);
        drop(profile);
        let rgba = color.straight_rgba_in(RgbColorSpace::Srgb);
        for (actual, expected) in rgba[..3].iter().zip([0.25, 0.5, 0.75]) {
            assert!((actual - expected).abs() < 0.01);
        }
        assert_eq!(rgba[3], 0.5);
        let lowered = crate::canvas_renderer::color_space::lower_straight_color(&color);
        assert_eq!(lowered[3], 0.5);
        for (actual, straight) in lowered[..3]
            .iter()
            .zip(color.straight_rgba_in(RgbColorSpace::LinearSrgb))
        {
            assert!((actual - straight * 0.5).abs() < 1e-5);
        }
    }

    #[test]
    fn icc_color_rejects_invalid_coordinates_and_implicit_absolute_conversion() {
        let bytes = moxcms::ColorProfile::new_srgb().encode().unwrap();
        let profile = IccProfile::from_bytes(&bytes).unwrap();
        let opacity = UnitF32::new(1.0).unwrap();
        assert!(matches!(
            CanvasColor::from_icc(
                &profile,
                vec![0.5, 0.5],
                opacity,
                IccRenderingIntent::RelativeColorimetric,
            ),
            Err(CanvasIccColorError::InvalidCoordinates)
        ));
        assert!(matches!(
            CanvasColor::from_icc(
                &profile,
                vec![0.5, 0.5, 0.5],
                opacity,
                IccRenderingIntent::AbsoluteColorimetric,
            ),
            Err(CanvasIccColorError::AbsoluteIntent)
        ));
    }

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
        let lowered_original = crate::canvas_renderer::color_space::lower_straight_color(&original);

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
            let lowered = crate::canvas_renderer::color_space::lower_straight_color(&canvas);
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
