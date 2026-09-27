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
pub trait ColorCoordinates<M: TypedColorModel>: 'static {}

/// Defines the structure and ordering of a color profile's coordinates.
///
/// A model assigns no physical color meaning by itself. Primaries, transfer
/// functions, white points, matrices, and lookup tables belong to a profile.
pub trait TypedColorModel: Sized + 'static {
    const COMPONENTS: usize;
    const ERASED: ColorModel;
}

/// The coordinate model retained after a typed color space is erased.
///
/// `Channels` preserves the arity of an ICC n-colour or application-defined
/// model without incorrectly treating its coordinates as RGB or another named
/// model of the same size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ColorModel {
    Rgb,
    Cmyk,
    Lab,
    Xyz,
    Ycbcr,
    Gray,
    Luv,
    Yxy,
    Hsv,
    Hls,
    Cmy,
    Channels(usize),
}

impl ColorModel {
    pub const fn channels(self) -> usize {
        match self {
            Self::Rgb
            | Self::Lab
            | Self::Xyz
            | Self::Ycbcr
            | Self::Luv
            | Self::Yxy
            | Self::Hsv
            | Self::Hls
            | Self::Cmy => 3,
            Self::Cmyk => 4,
            Self::Gray => 1,
            Self::Channels(channels) => channels,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn accepts<M: TypedColorModel, C: ColorCoordinates<M>>() {}

    #[test]
    fn models_accept_whole_coordinate_representations() {
        accepts::<Rgb, [u8; 3]>();
        accepts::<Cmyk, [f32; 4]>();
        assert_eq!(Rgb::COMPONENTS, 3);
        assert_eq!(Rgb::ERASED, ColorModel::Rgb);
    }
}
