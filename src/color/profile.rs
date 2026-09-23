use super::{ColorModel, Lab, Rgb, Xyz};

/// Gives a [`ColorModel`] physical color meaning and a path to a profile
/// connection space.
///
/// The concrete transform representation is intentionally added separately;
/// this trait establishes the compile-time relationship between a profile and
/// the coordinate model it accepts.
pub trait ColorProfile: 'static {
    type Model: ColorModel;
}

macro_rules! profile {
    ($name:ident, $model:ty) => {
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
        pub struct $name;

        impl ColorProfile for $name {
            type Model = $model;
        }
    };
}

profile!(Srgb, Rgb);
profile!(LinearSrgb, Rgb);
profile!(DisplayP3, Rgb);
profile!(LinearDisplayP3, Rgb);
profile!(XyzD50, Xyz);
profile!(XyzD65, Xyz);
profile!(LabD50, Lab);

/// A predefined RGB space in which Canvas color operations can be evaluated.
///
/// This runtime value is the erased counterpart of the zero-sized profile
/// types. It remains separate from sample representation: an operation space
/// says how coordinates are interpreted, not whether they arrived as `u8`,
/// `f32`, or another representation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RgbColorSpace {
    Srgb,
    LinearSrgb,
    DisplayP3,
    LinearDisplayP3,
}

impl RgbColorSpace {
    /// Whether straight RGB coordinates in this space are linear-light.
    pub const fn is_linear(self) -> bool {
        matches!(self, Self::LinearSrgb | Self::LinearDisplayP3)
    }
}

macro_rules! rgb_color_space {
    ($profile:ty, $variant:ident) => {
        impl From<$profile> for RgbColorSpace {
            fn from(_: $profile) -> Self {
                Self::$variant
            }
        }
    };
}

rgb_color_space!(Srgb, Srgb);
rgb_color_space!(LinearSrgb, LinearSrgb);
rgb_color_space!(DisplayP3, DisplayP3);
rgb_color_space!(LinearDisplayP3, LinearDisplayP3);

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_rgb_profile<P: ColorProfile<Model = Rgb>>() {}

    #[test]
    fn profiles_declare_their_coordinate_model() {
        assert_rgb_profile::<Srgb>();
        assert_rgb_profile::<LinearSrgb>();
        assert_rgb_profile::<DisplayP3>();
        assert_rgb_profile::<LinearDisplayP3>();
    }

    #[test]
    fn predefined_rgb_profiles_erase_to_operation_spaces() {
        assert_eq!(RgbColorSpace::from(Srgb), RgbColorSpace::Srgb);
        assert_eq!(
            RgbColorSpace::from(LinearDisplayP3),
            RgbColorSpace::LinearDisplayP3
        );
    }
}
