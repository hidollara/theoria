/// The meaning and order of a color space's input coordinates.
///
/// This does not define primaries, white points, component scales, encoding
/// precision, or alpha. Lab/Lch include CIE and Oklab/Oklch spaces; their
/// different coordinate meanings and scales are supplied by their profiles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ColorModel {
    Xyz,
    Lab,
    Lch,
    Luv,
    YCbCr,
    Yxy,
    Rgb,
    Gray,
    Hsv,
    Hls,
    Hwb,
    Cmyk,
    Cmy,
    Other {
        channels: usize,
    },
}

impl ColorModel {
    /// Index of a periodic channel, independently of its units or meaning.
    /// This does not prescribe normalization or an interpolation direction.
    pub const fn periodic_channel_index(self) -> Option<usize> {
        match self {
            Self::Lch => Some(2),
            Self::Hsv | Self::Hls | Self::Hwb => Some(0),
            _ => None,
        }
    }

    pub const fn channels(self) -> usize {
        match self {
            Self::Gray => 1,
            Self::Cmyk => 4,
            Self::Other { channels } => channels,
            _ => 3,
        }
    }
}

/// Marks a coordinate model for statically typed color spaces.
///
/// The associated constant links compile-time input constraints to the runtime
/// model exposed by a profile. A profile defines the colorimetric meaning of
/// those coordinates, not their Rust storage representation.
pub trait TypedColorModel: Sized + 'static {
    const MODEL: ColorModel;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Xyz;

impl TypedColorModel for Xyz {
    const MODEL: ColorModel = ColorModel::Xyz;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Lab;

impl TypedColorModel for Lab {
    const MODEL: ColorModel = ColorModel::Lab;
}

/// Lightness, chroma, and hue; scales are defined by the color space.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Lch;

impl TypedColorModel for Lch {
    const MODEL: ColorModel = ColorModel::Lch;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rgb;

impl TypedColorModel for Rgb {
    const MODEL: ColorModel = ColorModel::Rgb;
}

/// Hue, lightness, and saturation; units are defined by the color space.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Hls;

impl TypedColorModel for Hls {
    const MODEL: ColorModel = ColorModel::Hls;
}

/// Hue, whiteness, and blackness; units are defined by the color space.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Hwb;

impl TypedColorModel for Hwb {
    const MODEL: ColorModel = ColorModel::Hwb;
}
