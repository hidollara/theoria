use super::{ColorCoordinates, ColorModel};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Ycbcr;

impl ColorModel for Ycbcr {
    const COMPONENTS: usize = 3;
}

impl ColorCoordinates<Ycbcr> for [f32; 3] {}
impl ColorCoordinates<Ycbcr> for [f64; 3] {}
