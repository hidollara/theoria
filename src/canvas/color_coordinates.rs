use std::sync::Arc;

use crate::color::{Hls, Hwb, Lab, Lch, Rgb, TypedColorModel, Xyz};

use super::color::{Alpha, ColorValue};

/// A coordinate representation that Canvas can retain without color conversion.
/// Integer variants contain normalized unsigned code values; floats retain
/// the model's native coordinate values. Alpha is supplied separately.
#[derive(Clone, Debug, PartialEq)]
pub enum CoordinateValue {
    U8([u8; 3]),
    U16([u16; 3]),
    F32([f32; 3]),
    F64([f64; 3]),
    Dynamic(Arc<[f64]>),
}

impl CoordinateValue {
    pub(crate) fn with_alpha(self, alpha: Alpha) -> ColorValue {
        match self {
            Self::U8(value) => ColorValue::U8(value, alpha),
            Self::U16(value) => ColorValue::U16(value, alpha),
            Self::F32(value) => ColorValue::F32(value, alpha),
            Self::F64(value) => ColorValue::F64(value, alpha),
            Self::Dynamic(value) => ColorValue::Dynamic(value, alpha),
        }
    }

    pub(crate) fn channels(&self) -> usize {
        match self {
            Self::Dynamic(value) => value.len(),
            _ => 3,
        }
    }

    pub(crate) fn is_finite(&self) -> bool {
        match self {
            Self::U8(_) | Self::U16(_) => true,
            Self::F32(value) => value.iter().all(|v| v.is_finite()),
            Self::F64(value) => value.iter().all(|v| v.is_finite()),
            Self::Dynamic(value) => value.iter().all(|v| v.is_finite()),
        }
    }
}

/// A coordinate representation accepted by model `M` and retained by Canvas.
///
/// Implementations must return coordinates in the order and encoding expected
/// by `M`, with exactly `M::MODEL.channels()` components. This does not impose
/// a value range or perform color conversion. Types with value invariants
/// enforce them when their values are constructed.
///
/// A custom representation needs only this trait; no private erasure trait is
/// required. Its original values are kept until renderer preparation.
///
/// ```
/// use theoria::{
///     canvas::{Color, ColorCoordinates, CoordinateValue, Opacity},
///     color::{Rgb, Srgb},
/// };
/// struct Rgb8 { r: u8, g: u8, b: u8 }
/// impl ColorCoordinates<Rgb> for Rgb8 {
///     fn into_value(self) -> CoordinateValue {
///         CoordinateValue::U8([self.r, self.g, self.b])
///     }
/// }
/// let color = Color::new::<Srgb>(Srgb, Rgb8 { r: 255, g: 128, b: 64 }, Opacity::<u8>::ONE);
/// let runtime = Color::new_in_space::<Rgb>(Srgb, Rgb8 { r: 255, g: 128, b: 64 }, Opacity::<u8>::ONE).unwrap();
/// assert_eq!(color, runtime);
/// ```
pub trait ColorCoordinates<M: TypedColorModel>: 'static {
    fn into_value(self) -> CoordinateValue;
}

impl ColorCoordinates<Rgb> for [u8; 3] {
    fn into_value(self) -> CoordinateValue {
        CoordinateValue::U8(self)
    }
}

impl ColorCoordinates<Rgb> for [u16; 3] {
    fn into_value(self) -> CoordinateValue {
        CoordinateValue::U16(self)
    }
}

impl ColorCoordinates<Rgb> for [f32; 3] {
    fn into_value(self) -> CoordinateValue {
        CoordinateValue::F32(self)
    }
}

impl ColorCoordinates<Rgb> for [f64; 3] {
    fn into_value(self) -> CoordinateValue {
        CoordinateValue::F64(self)
    }
}

impl ColorCoordinates<Xyz> for [f32; 3] {
    fn into_value(self) -> CoordinateValue {
        CoordinateValue::F32(self)
    }
}

impl ColorCoordinates<Xyz> for [f64; 3] {
    fn into_value(self) -> CoordinateValue {
        CoordinateValue::F64(self)
    }
}

impl ColorCoordinates<Lab> for [f32; 3] {
    fn into_value(self) -> CoordinateValue {
        CoordinateValue::F32(self)
    }
}

impl ColorCoordinates<Lab> for [f64; 3] {
    fn into_value(self) -> CoordinateValue {
        CoordinateValue::F64(self)
    }
}

impl ColorCoordinates<Lch> for [f32; 3] {
    fn into_value(self) -> CoordinateValue {
        CoordinateValue::F32(self)
    }
}

impl ColorCoordinates<Lch> for [f64; 3] {
    fn into_value(self) -> CoordinateValue {
        CoordinateValue::F64(self)
    }
}

impl ColorCoordinates<Hls> for [f32; 3] {
    fn into_value(self) -> CoordinateValue {
        CoordinateValue::F32(self)
    }
}

impl ColorCoordinates<Hls> for [f64; 3] {
    fn into_value(self) -> CoordinateValue {
        CoordinateValue::F64(self)
    }
}

impl ColorCoordinates<Hwb> for [f32; 3] {
    fn into_value(self) -> CoordinateValue {
        CoordinateValue::F32(self)
    }
}

impl ColorCoordinates<Hwb> for [f64; 3] {
    fn into_value(self) -> CoordinateValue {
        CoordinateValue::F64(self)
    }
}

impl<M: TypedColorModel> ColorCoordinates<M> for Arc<[f64]> {
    fn into_value(self) -> CoordinateValue {
        CoordinateValue::Dynamic(self)
    }
}

impl<M: TypedColorModel> ColorCoordinates<M> for Vec<f64> {
    fn into_value(self) -> CoordinateValue {
        CoordinateValue::Dynamic(self.into())
    }
}
