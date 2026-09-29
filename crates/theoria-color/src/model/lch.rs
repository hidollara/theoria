use super::{ColorCoordinates, ColorModel, TypedColorModel};

/// CIE Lab lightness, chroma, and hue in degrees.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Lch;

impl TypedColorModel for Lch {
    const ERASED: ColorModel = ColorModel::Lch;
}

impl ColorCoordinates<Lch> for [f32; 3] {}
impl ColorCoordinates<Lch> for [f64; 3] {}
