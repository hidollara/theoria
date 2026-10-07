use std::{error::Error, fmt, sync::Arc};

use super::color_coordinates::ColorCoordinates;
use super::opacity::{Opacity, OpacitySample};
use crate::color::{
    ColorModel, ColorSpace, IccProfileError, SpaceConnectionError, TransferError, TypedColorModel,
    TypedColorSpace,
};

/// A Canvas color retaining its authored space and coordinate representation.
///
/// Construction erases the coordinate type without converting its values.
#[derive(Clone, Debug, PartialEq)]
pub struct Color {
    space: ColorSpace,
    value: ColorValue,
}

/// The authored storage of one color, including its opacity. The renderer
/// interprets the representation in the context of `Color::space`.
#[derive(Clone, Debug, PartialEq)]
pub enum ColorValue {
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

    pub fn alpha(&self) -> Alpha {
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
pub enum Alpha {
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

    pub fn as_f32(self) -> f32 {
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
    ///
    /// ```compile_fail
    /// use theoria_canvas::{canvas::{Color, Opacity}, color::LabD50};
    /// // Unsigned RGB code values are not Lab coordinates.
    /// Color::new(LabD50, [1_u8, 2, 3], Opacity::<u8>::ONE);
    /// ```
    #[allow(private_bounds)]
    pub fn new<S: TypedColorSpace>(
        space: S,
        coordinates: impl ColorCoordinates<S::Model>,
        opacity: Opacity<impl CanvasOpacity>,
    ) -> Self {
        let coordinates = coordinates.into_value();
        assert_eq!(
            coordinates.channels(),
            S::Model::MODEL.channels(),
            "coordinates must match their declared model"
        );
        Self {
            space: space.into(),
            value: coordinates.with_alpha(erase_opacity(opacity)),
        }
    }

    /// The authored color and opacity storage, without conversion.
    pub const fn value(&self) -> &ColorValue {
        &self.value
    }

    pub const fn space(&self) -> &ColorSpace {
        &self.space
    }

    /// Keeps authored device coordinates and their selected color space.
    /// The renderer converts them into its working space when needed.
    #[allow(private_bounds)]
    pub fn new_in_space<M: TypedColorModel>(
        space: impl Into<ColorSpace>,
        coordinates: impl ColorCoordinates<M>,
        opacity: Opacity<impl CanvasOpacity>,
    ) -> Result<Self, ColorError> {
        let space = space.into();
        if space.model() != M::MODEL {
            return Err(ColorError::ModelMismatch {
                expected: space.model(),
                actual: M::MODEL,
            });
        }
        let coordinates = coordinates.into_value();
        if coordinates.channels() != space.channels() || !coordinates.is_finite() {
            return Err(ColorError::InvalidCoordinates);
        }
        Ok(Self {
            space,
            value: coordinates.with_alpha(erase_opacity(opacity)),
        })
    }

    pub fn with_alpha_factor(&self, factor: f32) -> Self {
        Self {
            space: self.space.clone(),
            value: self.value.with_alpha_factor(factor),
        }
    }
}

fn erase_opacity<A: CanvasOpacity>(opacity: Opacity<A>) -> Alpha {
    A::erase(opacity)
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
    ModelMismatch {
        expected: ColorModel,
        actual: ColorModel,
    },
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
            Self::ModelMismatch { expected, actual } => write!(
                formatter,
                "color space requires {expected:?} coordinates, got {actual:?}"
            ),
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
