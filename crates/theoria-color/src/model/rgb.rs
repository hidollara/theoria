use super::{ColorCoordinates, TypedColorModel};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rgb;

impl TypedColorModel for Rgb {}

impl ColorCoordinates<Rgb> for [u8; 3] {}
impl ColorCoordinates<Rgb> for [u16; 3] {}
impl ColorCoordinates<Rgb> for [f32; 3] {}
impl ColorCoordinates<Rgb> for [f64; 3] {}
