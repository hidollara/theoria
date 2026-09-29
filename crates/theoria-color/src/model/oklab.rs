use super::{ColorCoordinates, ColorModel, TypedColorModel};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Oklab;

impl TypedColorModel for Oklab {
    const ERASED: ColorModel = ColorModel::Oklab;
}

impl ColorCoordinates<Oklab> for [f32; 3] {}
impl ColorCoordinates<Oklab> for [f64; 3] {}
