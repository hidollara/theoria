use super::{ColorCoordinates, ColorModel, TypedColorModel};

/// Hue (degrees), saturation (percent), and lightness (percent).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Hsl;

impl TypedColorModel for Hsl {
    const ERASED: ColorModel = ColorModel::Hsl;
}

impl ColorCoordinates<Hsl> for [f32; 3] {}
impl ColorCoordinates<Hsl> for [f64; 3] {}
