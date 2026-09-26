use super::{ColorCoordinates, ColorModel};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Cmyk;

impl ColorModel for Cmyk {
    const COMPONENTS: usize = 4;
}

impl ColorCoordinates<Cmyk> for [f32; 4] {}
impl ColorCoordinates<Cmyk> for [f64; 4] {}
