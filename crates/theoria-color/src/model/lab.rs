use super::{ColorCoordinates, ColorModel};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Lab;

impl ColorModel for Lab {
    const COMPONENTS: usize = 3;
}

impl ColorCoordinates<Lab> for [f32; 3] {}
impl ColorCoordinates<Lab> for [f64; 3] {}
