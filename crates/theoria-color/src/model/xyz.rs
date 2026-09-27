use super::{ColorCoordinates, ColorModel, TypedColorModel};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Xyz;

impl TypedColorModel for Xyz {
    const COMPONENTS: usize = 3;
    const ERASED: ColorModel = ColorModel::Xyz;
}

impl ColorCoordinates<Xyz> for [f32; 3] {}
impl ColorCoordinates<Xyz> for [f64; 3] {}
