use super::{ColorCoordinates, TypedColorModel};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Xyz;

impl TypedColorModel for Xyz {}

impl ColorCoordinates<Xyz> for [f32; 3] {}
impl ColorCoordinates<Xyz> for [f64; 3] {}
