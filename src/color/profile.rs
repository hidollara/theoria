use super::profile_data::{
    LINEAR_P3_TO_XYZ_D65, LINEAR_SRGB_TO_XYZ_D65, XYZ_D65_TO_LINEAR_P3, XYZ_D65_TO_LINEAR_SRGB,
};
use super::{ColorModel, Lab, Rgb, RgbTransfer, RgbTransform, Xyz};

/// Associates a profile with the coordinate model it accepts.
///
/// Conversion capabilities are specified separately; merely naming a model
/// does not imply that a renderer can convert an arbitrary profile.
pub trait ColorProfile: 'static {
    type Model: ColorModel;
}

/// An RGB profile represented by a transfer curve and a pair of XYZ D65 matrices.
///
/// Profiles needing a different transform representation are not covered by
/// this capability. XYZ D65 is the shared space of the current RGB profiles,
/// not a claim that it is the ICC profile connection space.
pub trait MatrixRgbProfile: ColorProfile<Model = Rgb> {
    const TRANSFORM: RgbTransform;
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

impl MatrixRgbProfile for Srgb {
    const TRANSFORM: RgbTransform = RgbTransform::new(
        RgbTransfer::Srgb,
        LINEAR_SRGB_TO_XYZ_D65,
        XYZ_D65_TO_LINEAR_SRGB,
    );
}

impl MatrixRgbProfile for LinearSrgb {
    const TRANSFORM: RgbTransform = RgbTransform::new(
        RgbTransfer::Linear,
        LINEAR_SRGB_TO_XYZ_D65,
        XYZ_D65_TO_LINEAR_SRGB,
    );
}

impl MatrixRgbProfile for DisplayP3 {
    const TRANSFORM: RgbTransform = RgbTransform::new(
        RgbTransfer::Srgb,
        LINEAR_P3_TO_XYZ_D65,
        XYZ_D65_TO_LINEAR_P3,
    );
}

impl MatrixRgbProfile for LinearDisplayP3 {
    const TRANSFORM: RgbTransform = RgbTransform::new(
        RgbTransfer::Linear,
        LINEAR_P3_TO_XYZ_D65,
        XYZ_D65_TO_LINEAR_P3,
    );
}

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

impl ColorProfile for RgbColorSpace {
    type Model = Rgb;
}

impl RgbColorSpace {
    /// Whether straight RGB coordinates in this space are linear-light.
    pub const fn is_linear(self) -> bool {
        matches!(self.transform().transfer(), RgbTransfer::Linear)
    }

    /// The conversion data of this predefined space.
    pub const fn transform(self) -> RgbTransform {
        match self {
            Self::Srgb => Srgb::TRANSFORM,
            Self::LinearSrgb => LinearSrgb::TRANSFORM,
            Self::DisplayP3 => DisplayP3::TRANSFORM,
            Self::LinearDisplayP3 => LinearDisplayP3::TRANSFORM,
        }
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

    #[test]
    fn erased_spaces_use_their_profile_transform() {
        assert_eq!(RgbColorSpace::Srgb.transform(), Srgb::TRANSFORM);
        assert_eq!(RgbColorSpace::LinearSrgb.transform(), LinearSrgb::TRANSFORM);
        assert_eq!(RgbColorSpace::DisplayP3.transform(), DisplayP3::TRANSFORM);
        assert_eq!(
            RgbColorSpace::LinearDisplayP3.transform(),
            LinearDisplayP3::TRANSFORM
        );
    }

    #[test]
    fn rgb_profiles_round_trip_through_xyz_d65() {
        fn check<P: MatrixRgbProfile>() {
            let rgb = [0.23, 0.51, 0.79];
            let xyz = P::TRANSFORM.rgb_to_xyz_d65(rgb);
            let actual = P::TRANSFORM.xyz_d65_to_rgb(xyz);
            for (actual, expected) in actual.into_iter().zip(rgb) {
                assert!((actual - expected).abs() < 0.000_01);
            }
        }

        check::<Srgb>();
        check::<LinearSrgb>();
        check::<DisplayP3>();
        check::<LinearDisplayP3>();
    }
}
