mod hsl;
mod hwb;
mod lab;
mod lch;
mod oklab;
mod oklch;
mod rgb;
mod xyz;

pub use hsl::Hsl;
pub use hwb::Hwb;
pub use lab::Lab;
pub use lch::Lch;
pub use oklab::Oklab;
pub use oklch::Oklch;
pub use rgb::Rgb;
pub use xyz::Xyz;

/// An authored coordinate representation accepted by a color model.
///
/// This does not impose a value range. Representations with invariants enforce
/// them when their values are constructed.
pub trait ColorCoordinates<M: TypedColorModel>: 'static {}

/// Marks a coordinate model for statically typed color spaces.
///
/// The model groups compatible coordinate representations; a profile defines
/// their colorimetric interpretation. Its exact role remains independent of
/// the runtime metadata exposed by [`crate::space::ColorSpace`].
pub trait TypedColorModel: Sized + 'static {}

#[cfg(test)]
mod tests {
    use super::*;

    fn accepts<M: TypedColorModel, C: ColorCoordinates<M>>() {}

    #[test]
    fn models_accept_whole_coordinate_representations() {
        accepts::<Rgb, [u8; 3]>();
        accepts::<Lab, [f32; 3]>();
    }
}
