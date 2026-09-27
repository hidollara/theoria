use super::{ColorCoordinates, ColorModel, TypedColorModel};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Ycbcr;

impl TypedColorModel for Ycbcr {
    const COMPONENTS: usize = 3;
    const ERASED: ColorModel = ColorModel::Ycbcr;
}

impl ColorCoordinates<Ycbcr> for [f32; 3] {}
impl ColorCoordinates<Ycbcr> for [f64; 3] {}
