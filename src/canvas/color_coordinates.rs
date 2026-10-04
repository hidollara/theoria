use crate::color::{Hsl, Hwb, Lab, Lch, Oklab, Oklch, Rgb, TypedColorModel, Xyz};

/// An authored coordinate representation accepted by a color model in Canvas.
///
/// This does not impose a value range or convert coordinates. Representations
/// with invariants enforce them when their values are constructed.
pub trait ColorCoordinates<M: TypedColorModel>: 'static {}

impl ColorCoordinates<Rgb> for [u8; 3] {}
impl ColorCoordinates<Rgb> for [u16; 3] {}
impl ColorCoordinates<Rgb> for [f32; 3] {}
impl ColorCoordinates<Rgb> for [f64; 3] {}

impl ColorCoordinates<Xyz> for [f32; 3] {}
impl ColorCoordinates<Xyz> for [f64; 3] {}

impl ColorCoordinates<Lab> for [f32; 3] {}
impl ColorCoordinates<Lab> for [f64; 3] {}

impl ColorCoordinates<Lch> for [f32; 3] {}
impl ColorCoordinates<Lch> for [f64; 3] {}

impl ColorCoordinates<Hsl> for [f32; 3] {}
impl ColorCoordinates<Hsl> for [f64; 3] {}

impl ColorCoordinates<Hwb> for [f32; 3] {}
impl ColorCoordinates<Hwb> for [f64; 3] {}

impl ColorCoordinates<Oklab> for [f32; 3] {}
impl ColorCoordinates<Oklab> for [f64; 3] {}

impl ColorCoordinates<Oklch> for [f32; 3] {}
impl ColorCoordinates<Oklch> for [f64; 3] {}
