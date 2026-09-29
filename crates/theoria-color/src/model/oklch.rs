use super::{ColorCoordinates, ColorModel, TypedColorModel};

/// Oklab lightness, chroma, and hue in degrees.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Oklch;

impl TypedColorModel for Oklch {
    const ERASED: ColorModel = ColorModel::Oklch;
}

impl ColorCoordinates<Oklch> for [f32; 3] {}
impl ColorCoordinates<Oklch> for [f64; 3] {}
