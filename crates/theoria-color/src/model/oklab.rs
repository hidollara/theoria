use super::{ColorCoordinates, TypedColorModel};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Oklab;

impl TypedColorModel for Oklab {}

impl ColorCoordinates<Oklab> for [f32; 3] {}
impl ColorCoordinates<Oklab> for [f64; 3] {}
