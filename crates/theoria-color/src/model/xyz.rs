use super::{ColorCoordinates, ColorModel};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Xyz;

impl ColorModel for Xyz {
    const COMPONENTS: usize = 3;
}

impl ColorCoordinates<Xyz> for [f32; 3] {}
impl ColorCoordinates<Xyz> for [f64; 3] {}
