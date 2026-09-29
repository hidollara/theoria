use super::{ColorCoordinates, TypedColorModel};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Lab;

impl TypedColorModel for Lab {}

impl ColorCoordinates<Lab> for [f32; 3] {}
impl ColorCoordinates<Lab> for [f64; 3] {}
