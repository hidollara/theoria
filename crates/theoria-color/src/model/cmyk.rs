use super::{ColorCoordinates, ColorModel, TypedColorModel};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Cmyk;

impl TypedColorModel for Cmyk {
    const COMPONENTS: usize = 4;
    const ERASED: ColorModel = ColorModel::Cmyk;
}

impl ColorCoordinates<Cmyk> for [f32; 4] {}
impl ColorCoordinates<Cmyk> for [f64; 4] {}
