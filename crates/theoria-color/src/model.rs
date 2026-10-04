/// Marks a coordinate model for statically typed color spaces.
///
/// The model groups compatible coordinate representations; a profile defines
/// their colorimetric interpretation. Its exact role remains independent of
/// the runtime metadata exposed by [`crate::space::ColorSpace`].
pub trait TypedColorModel: Sized + 'static {}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rgb;

impl TypedColorModel for Rgb {}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Xyz;

impl TypedColorModel for Xyz {}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Lab;

impl TypedColorModel for Lab {}

/// CIE Lab lightness, chroma, and hue in degrees.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Lch;

impl TypedColorModel for Lch {}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Oklab;

impl TypedColorModel for Oklab {}

/// Oklab lightness, chroma, and hue in degrees.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Oklch;

impl TypedColorModel for Oklch {}

/// Hue (degrees), saturation (percent), and lightness (percent).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Hsl;

impl TypedColorModel for Hsl {}

/// Hue (degrees), whiteness (percent), and blackness (percent).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Hwb;

impl TypedColorModel for Hwb {}
