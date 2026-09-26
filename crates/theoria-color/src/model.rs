mod cmyk;
mod lab;
mod rgb;
mod xyz;
mod ycbcr;

pub use cmyk::Cmyk;
pub use lab::Lab;
pub use rgb::Rgb;
pub use xyz::Xyz;
pub use ycbcr::Ycbcr;

/// An authored coordinate representation accepted by a color model.
///
/// This does not impose a value range. Representations with invariants enforce
/// them when their values are constructed.
pub trait ColorCoordinates<M: ColorModel>: 'static {}

/// Defines the structure and ordering of a color profile's coordinates.
///
/// A model assigns no physical color meaning by itself. Primaries, transfer
/// functions, white points, matrices, and lookup tables belong to a profile.
pub trait ColorModel: Sized + 'static {
    const COMPONENTS: usize;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn accepts<M: ColorModel, C: ColorCoordinates<M>>() {}

    #[test]
    fn models_accept_whole_coordinate_representations() {
        accepts::<Rgb, [u8; 3]>();
        accepts::<Cmyk, [f32; 4]>();
        assert_eq!(Rgb::COMPONENTS, 3);
    }
}
