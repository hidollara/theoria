/// The meaning and order of a color space's input coordinates.
///
/// This does not define primaries, white points, encoding precision, or alpha.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ColorModel {
    Rgb,
    Xyz,
    Lab,
    Lch,
    Oklab,
    Oklch,
    Hsl,
    Hwb,
    Gray,
    Cmy,
    Cmyk,
    Luv,
    Yxy,
    YCbCr,
    Hsv,
    /// Hue, lightness, saturation (distinct from HSL's channel order).
    Hls,
    /// Channels whose individual meanings are defined by the profile.
    Other {
        channels: usize,
    },
}

impl ColorModel {
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
pub struct Rgb;

impl TypedColorModel for Rgb {
    const MODEL: ColorModel = ColorModel::Rgb;
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

/// CIE Lab lightness, chroma, and hue in degrees.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Lch;

impl TypedColorModel for Lch {
    const MODEL: ColorModel = ColorModel::Lch;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Oklab;

impl TypedColorModel for Oklab {
    const MODEL: ColorModel = ColorModel::Oklab;
}

/// Oklab lightness, chroma, and hue in degrees.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Oklch;

impl TypedColorModel for Oklch {
    const MODEL: ColorModel = ColorModel::Oklch;
}

/// Hue (degrees), saturation (percent), and lightness (percent).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Hsl;

impl TypedColorModel for Hsl {
    const MODEL: ColorModel = ColorModel::Hsl;
}

/// Hue (degrees), whiteness (percent), and blackness (percent).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Hwb;

impl TypedColorModel for Hwb {
    const MODEL: ColorModel = ColorModel::Hwb;
}
