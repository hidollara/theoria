use super::{ColorCoordinates, TypedColorModel};

/// Hue (degrees), whiteness (percent), and blackness (percent).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Hwb;

impl TypedColorModel for Hwb {}

impl ColorCoordinates<Hwb> for [f32; 3] {}
impl ColorCoordinates<Hwb> for [f64; 3] {}
