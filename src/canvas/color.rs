use std::{error::Error, fmt, sync::Arc};

use super::opacity::{Opacity, OpacitySample};
use crate::color::{
    ColorCoordinates, ColorSpace, IccProfileError, SpaceConnectionError, TransferError,
    TypedColorSpace,
};

/// A Canvas color retaining its authored space and coordinate representation.
///
/// Construction erases the coordinate type without converting its values.
#[derive(Clone, Debug, PartialEq)]
pub struct Color {
    pub(crate) space: ColorSpace,
    pub(crate) value: ColorValue,
}

/// The authored storage of one color, including its opacity. The renderer
/// interprets the representation in the context of `Color::space`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ColorValue {
    F32([f32; 3], Alpha),
    F64([f64; 3], Alpha),
    U8([u8; 3], Alpha),
    U16([u16; 3], Alpha),
    Dynamic(Arc<[f64]>, Alpha),
}

impl ColorValue {
    fn with_alpha_factor(&self, factor: f32) -> Self {
        let alpha = self.alpha().with_factor(factor);
        match self {
            Self::F32(coordinates, _) => Self::F32(*coordinates, alpha),
            Self::F64(coordinates, _) => Self::F64(*coordinates, alpha),
            Self::U8(coordinates, _) => Self::U8(*coordinates, alpha),
            Self::U16(coordinates, _) => Self::U16(*coordinates, alpha),
            Self::Dynamic(coordinates, _) => Self::Dynamic(coordinates.clone(), alpha),
        }
    }

    pub(crate) fn alpha(&self) -> Alpha {
        match self {
            Self::F32(_, alpha)
            | Self::F64(_, alpha)
            | Self::U8(_, alpha)
            | Self::U16(_, alpha)
            | Self::Dynamic(_, alpha) => *alpha,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Alpha {
    F32(f32),
    F64(f64),
    U8(u8),
    U16(u16),
}

impl Alpha {
    fn with_factor(self, factor: f32) -> Self {
        let factor = factor.clamp(0.0, 1.0);
        match self {
            Self::F32(opacity) => Self::F32(
                Opacity::<f32>::new(opacity * factor)
                    .expect("a finite unit opacity remains valid after multiplication")
                    .sample(),
            ),
            Self::F64(opacity) => Self::F64(
                Opacity::<f64>::new(opacity * f64::from(factor))
                    .expect("a finite unit opacity remains valid after multiplication")
                    .sample(),
            ),
            Self::U8(opacity) => Self::F32(
                Opacity::<f32>::new(f32::from(opacity) / 255.0 * factor)
                    .expect("a finite unit opacity remains valid after multiplication")
                    .sample(),
            ),
            Self::U16(opacity) => Self::F32(
                Opacity::<f32>::new(f32::from(opacity) / f32::from(u16::MAX) * factor)
                    .expect("a finite unit opacity remains valid after multiplication")
                    .sample(),
            ),
        }
    }

    pub(crate) fn as_f32(self) -> f32 {
        match self {
            Self::F32(opacity) => opacity,
            Self::F64(opacity) => opacity as f32,
            Self::U8(opacity) => f32::from(opacity) / f32::from(u8::MAX),
            Self::U16(opacity) => f32::from(opacity) / f32::from(u16::MAX),
        }
    }
}

impl Color {
    /// Retains the authored coordinates and opacity without converting color spaces.
    /// Only coordinate representations supported by Canvas can be constructed.
    #[allow(private_bounds)]
    pub fn new<S, C, A>(space: S, coordinates: C, opacity: Opacity<A>) -> Self
    where
        S: TypedColorSpace,
        C: ColorCoordinates<S::Model> + CanvasCoordinates,
        A: CanvasOpacity,
    {
        Self {
            space: space.into(),
            value: coordinates.erase(A::erase(opacity)),
        }
    }

    pub const fn space(&self) -> &ColorSpace {
        &self.space
    }

    /// Keeps authored device coordinates and their selected color space.
    /// The renderer converts them into its working space when needed.
    pub fn new_in_space(
        space: impl Into<ColorSpace>,
        coordinates: impl Into<Arc<[f64]>>,
        opacity: Opacity<f32>,
    ) -> Result<Self, ColorError> {
        let space = space.into();
        let coordinates = coordinates.into();
        if coordinates.len() != space.channels()
            || coordinates.iter().any(|value| !value.is_finite())
        {
            return Err(ColorError::InvalidCoordinates);
        }
        Ok(Self {
            space,
            value: ColorValue::Dynamic(coordinates, Alpha::F32(opacity.sample())),
        })
    }

    pub(crate) fn with_alpha_factor(&self, factor: f32) -> Self {
        Self {
            space: self.space.clone(),
            value: self.value.with_alpha_factor(factor),
        }
    }
}

trait CanvasCoordinates {
    fn erase(self, alpha: Alpha) -> ColorValue;
}

impl CanvasCoordinates for [f32; 3] {
    fn erase(self, alpha: Alpha) -> ColorValue {
        ColorValue::F32(self, alpha)
    }
}

impl CanvasCoordinates for [f64; 3] {
    fn erase(self, alpha: Alpha) -> ColorValue {
        ColorValue::F64(self, alpha)
    }
}

impl CanvasCoordinates for [u8; 3] {
    fn erase(self, alpha: Alpha) -> ColorValue {
        ColorValue::U8(self, alpha)
    }
}

impl CanvasCoordinates for [u16; 3] {
    fn erase(self, alpha: Alpha) -> ColorValue {
        ColorValue::U16(self, alpha)
    }
}

trait CanvasOpacity: OpacitySample {
    fn erase(opacity: Opacity<Self>) -> Alpha;
}

impl CanvasOpacity for f32 {
    fn erase(opacity: Opacity<Self>) -> Alpha {
        Alpha::F32(opacity.sample())
    }
}

impl CanvasOpacity for f64 {
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

impl From<&Color> for Color {
    fn from(color: &Color) -> Self {
        color.clone()
    }
}

#[derive(Debug)]
pub enum ColorError {
    InvalidCoordinates,
    UnsupportedColorChannels(usize),
    Profile(IccProfileError),
    Connection(SpaceConnectionError),
    Transfer(TransferError),
}

impl From<IccProfileError> for ColorError {
    fn from(error: IccProfileError) -> Self {
        Self::Profile(error)
    }
}

impl From<TransferError> for ColorError {
    fn from(error: TransferError) -> Self {
        Self::Transfer(error)
    }
}

impl From<SpaceConnectionError> for ColorError {
    fn from(error: SpaceConnectionError) -> Self {
        Self::Connection(error)
    }
}

impl fmt::Display for ColorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCoordinates => formatter.write_str("color coordinates are invalid"),
            Self::UnsupportedColorChannels(channels) => write!(
                formatter,
                "gradient interpolation requires three color channels, got {channels}"
            ),
            Self::Profile(error) => error.fmt(formatter),
            Self::Connection(error) => error.fmt(formatter),
            Self::Transfer(error) => error.fmt(formatter),
        }
    }
}

impl Error for ColorError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Profile(error) => Some(error),
            Self::Connection(error) => Some(error),
            Self::Transfer(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::{
        DisplayP3, IccProfile, IccRenderingIntent, LabD50, LinearDisplayP3, LinearSrgb, Srgb,
        XyzD50, XyzD65,
    };

    // CSS Color 4 §10.9 gives these equivalent coordinates for #7654CD.
    // https://www.w3.org/TR/css-color-4/#predefined
    const CSS_SRGB: [f32; 3] = [118.0 / 255.0, 84.0 / 255.0, 205.0 / 255.0];
    const CSS_XYZ_D50: [f64; 3] = [0.2005, 0.14089, 0.4472];
    const CSS_XYZ_D65: [f64; 3] = [0.21661, 0.14602, 0.59452];
    const CSS_LAB_D50: [f64; 3] = [44.36, 36.05, -58.99];

    #[test]
    fn icc_color_preserves_device_coordinates_until_renderer_lowering() {
        let bytes = moxcms::ColorProfile::new_srgb().encode().unwrap();
        let profile = IccProfile::from_bytes(&bytes).unwrap();
        let coordinates: Arc<[f64]> = vec![0.25, 0.5, 0.75].into();
        let color = Color::new_in_space(
            profile.color_space(IccRenderingIntent::RelativeColorimetric),
            coordinates.clone(),
            Opacity::<f32>::new(0.5).unwrap(),
        )
        .unwrap();
        let ColorValue::Dynamic(stored, Alpha::F32(stored_opacity)) = &color.value else {
            panic!("ICC device coordinates were converted at the Canvas boundary")
        };
        assert!(Arc::ptr_eq(stored, &coordinates));
        assert_eq!(*stored_opacity, 0.5);
        assert_eq!(
            color.space,
            profile.color_space(IccRenderingIntent::RelativeColorimetric)
        );
        drop(profile);
        let rgba = color.straight_rgba_in(Srgb);
        for (actual, expected) in rgba[..3].iter().zip([0.25, 0.5, 0.75]) {
            assert!((actual - expected).abs() < 0.01);
        }
        assert_eq!(rgba[3], 0.5);
        let lowered = crate::canvas_renderer::color_space::lower_straight_color(&color);
        assert_eq!(lowered[3], 0.5);
        for (actual, straight) in lowered[..3].iter().zip(color.straight_rgba_in(LinearSrgb)) {
            assert!((actual - straight * 0.5).abs() < 1e-5);
        }
    }

    #[test]
    fn icc_colors_from_separately_loaded_spaces_are_not_equal() {
        let bytes = moxcms::ColorProfile::new_srgb().encode().unwrap();
        let first = IccProfile::from_bytes(&bytes).unwrap();
        let second = IccProfile::from_bytes(&bytes).unwrap();
        let color = |profile: &IccProfile| {
            Color::new_in_space(
                profile.color_space(IccRenderingIntent::RelativeColorimetric),
                vec![0.25, 0.5, 0.75],
                Opacity::<f32>::ONE,
            )
            .unwrap()
        };
        assert_eq!(color(&first), color(&first));
        assert_ne!(color(&first), color(&second));
    }

    #[test]
    fn icc_color_rejects_invalid_coordinates() {
        let bytes = moxcms::ColorProfile::new_srgb().encode().unwrap();
        let profile = IccProfile::from_bytes(&bytes).unwrap();
        let opacity = Opacity::<f32>::ONE;
        assert!(matches!(
            Color::new_in_space(
                profile.color_space(IccRenderingIntent::RelativeColorimetric),
                vec![0.5, 0.5],
                opacity,
            ),
            Err(ColorError::InvalidCoordinates)
        ));
    }

    #[test]
    fn constructing_a_color_does_not_normalize_its_storage() {
        let color = Color::new(Srgb, [255_u8, 128, 64], Opacity::<u8>::from(128));

        assert!(matches!(
            color.value,
            ColorValue::U8([255, 128, 64], Alpha::U8(128))
        ));
        assert_eq!(color.space, ColorSpace::from(Srgb));
        assert_eq!(
            color.straight_rgba_in(Srgb),
            [1.0, 128.0 / 255.0, 64.0 / 255.0, 128.0 / 255.0]
        );
    }

    #[test]
    fn constructing_a_color_retains_its_rgb_space() {
        let color = Color::new(
            DisplayP3,
            [1.0, 0.5, 0.25],
            Opacity::<f32>::new(0.75).unwrap(),
        );

        assert_eq!(color.space, ColorSpace::from(DisplayP3));
        assert_eq!(color.straight_rgba_in(DisplayP3), [1.0, 0.5, 0.25, 0.75]);
    }

    #[test]
    fn changing_opacity_preserves_authored_coordinates_and_space() {
        let color = Color::new(LinearDisplayP3, [255_u8, 128, 64], Opacity::from(128_u8));

        let adjusted = color.with_alpha_factor(0.5);

        assert!(matches!(adjusted.value, ColorValue::U8([255, 128, 64], _)));
        assert_eq!(adjusted.space, ColorSpace::from(LinearDisplayP3));
        assert!((adjusted.value.alpha().as_f32() - 64.0 / 255.0).abs() < 0.000_001);
    }

    #[test]
    fn linear_coordinates_enter_canvas_without_losing_storage() {
        let color = Color::new(
            LinearSrgb,
            [0.05_f64, 0.2, 0.5],
            Opacity::<f32>::new(0.4).unwrap(),
        );

        assert!(matches!(color.value, ColorValue::F64(_, Alpha::F32(_))));
        assert_eq!(color.space, ColorSpace::from(LinearSrgb));
        let rgba = color.straight_rgba_in(LinearSrgb);
        for (actual, expected) in rgba.into_iter().zip([0.05, 0.2, 0.5, 0.4]) {
            assert!((actual - expected).abs() < 0.000_001);
        }
    }

    #[test]
    fn color_value_keeps_coordinate_and_opacity_representations_together() {
        let color = Color::new(
            DisplayP3,
            [0.125_f64, 0.25, 0.5],
            Opacity::<f64>::new(0.75).unwrap(),
        );
        assert!(matches!(color.value, ColorValue::F64(_, Alpha::F64(_))));
        assert!(matches!(
            color.with_alpha_factor(0.5).value,
            ColorValue::F64(_, Alpha::F64(_))
        ));

        let color = Color::new(Srgb, [0_u16, u16::MAX / 2, u16::MAX], Opacity::from(128_u8));
        assert!(matches!(color.value, ColorValue::U16(_, Alpha::U8(_))));
    }

    #[test]
    fn xyz_and_lab_colors_remain_authored_until_renderer_lowering() {
        let opacity = Opacity::<f32>::new(0.4).unwrap();
        let original = Color::new(Srgb, CSS_SRGB, opacity);
        let lowered_original = crate::canvas_renderer::color_space::lower_straight_color(&original);

        for (canvas, space) in [
            (
                Color::new(XyzD50, CSS_XYZ_D50, opacity),
                ColorSpace::from(XyzD50),
            ),
            (
                Color::new(XyzD65, CSS_XYZ_D65, opacity),
                ColorSpace::from(XyzD65),
            ),
            (
                Color::new(LabD50, CSS_LAB_D50, opacity),
                ColorSpace::from(LabD50),
            ),
        ] {
            assert_eq!(canvas.space, space);
            assert!(matches!(canvas.value, ColorValue::F64(_, Alpha::F32(_))));
            let rgba = canvas.straight_rgba_in(Srgb);
            for (actual, expected) in
                rgba.into_iter()
                    .zip([CSS_SRGB[0], CSS_SRGB[1], CSS_SRGB[2], 0.4])
            {
                assert!((actual - expected).abs() < 0.002, "{actual} != {expected}");
            }
            let lowered = crate::canvas_renderer::color_space::lower_straight_color(&canvas);
            for (actual, expected) in lowered.into_iter().zip(lowered_original) {
                assert!((actual - expected).abs() < 0.001, "{actual} != {expected}");
            }
        }
    }

    #[test]
    fn lab_color_can_be_used_as_a_gradient_stop() {
        let lab = Color::new(LabD50, CSS_LAB_D50, Opacity::<f32>::new(0.4).unwrap());
        let stop = crate::canvas::GradientStop::new(0.5, lab).unwrap();

        let rgba = stop.color().straight_rgba_in(Srgb);
        for (actual, expected) in rgba
            .into_iter()
            .zip([CSS_SRGB[0], CSS_SRGB[1], CSS_SRGB[2], 0.4])
        {
            assert!((actual - expected).abs() < 0.002, "{actual} != {expected}");
        }
    }
}
