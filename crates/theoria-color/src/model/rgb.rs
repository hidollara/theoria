use super::{ColorCoordinates, ColorModel};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rgb;

impl ColorModel for Rgb {
    const COMPONENTS: usize = 3;
}

impl ColorCoordinates<Rgb> for [u8; 3] {}
impl ColorCoordinates<Rgb> for [u16; 3] {}
impl ColorCoordinates<Rgb> for [f32; 3] {}
impl ColorCoordinates<Rgb> for [f64; 3] {}
