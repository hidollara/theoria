use super::{ColorCoordinates, ColorModel, TypedColorModel};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Lab;

impl TypedColorModel for Lab {
    const ERASED: ColorModel = ColorModel::Lab;
}

impl ColorCoordinates<Lab> for [f32; 3] {}
impl ColorCoordinates<Lab> for [f64; 3] {}
