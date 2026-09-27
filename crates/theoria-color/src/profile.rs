use std::{
    fmt,
    hash::{Hash, Hasher},
    num::NonZeroUsize,
    sync::Arc,
};

use super::{ColorModel, Rgb, RgbTransfer, RgbTransform};

/// Associates a named color space with its coordinate model.
///
/// This is a compile-time constraint on color values. It does not promise a
/// conversion: that is supplied independently by [`crate::ColorProfile`].
pub trait ColorSpace: 'static {
    type Model: ColorModel;
}

/// An RGB profile represented by a transfer curve and a pair of XYZ D65 matrices.
///
/// Profiles needing a different transform representation are not covered by
/// this capability. XYZ D65 is the shared space of the current RGB profiles,
/// not a claim that it is the ICC profile connection space.
pub trait MatrixRgbProfile: ColorSpace<Model = Rgb> {
    const TRANSFORM: RgbTransform;
}

macro_rules! profile {
    ($name:ident, $model:ty) => {
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
        pub struct $name;

        impl ColorSpace for $name {
            type Model = $model;
        }
    };
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

impl ColorSpace for RgbColorSpace {
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

#[derive(Debug)]
struct ColorSpaceIdentity {
    channels: NonZeroUsize,
}

/// An opaque identity for one coordinate space, independent of its profile data.
///
/// Clone a key to label another transform with the same coordinates. Creating
/// another key, even with the same channel count, denotes a different space.
/// A key keeps only this small identity alive, not an ICC profile or its LUTs.
#[derive(Clone)]
pub struct ColorSpaceKey(Arc<ColorSpaceIdentity>);

impl ColorSpaceKey {
    /// Creates a distinct space with the specified number of channels.
    pub fn new(channels: NonZeroUsize) -> Self {
        Self(Arc::new(ColorSpaceIdentity { channels }))
    }

    pub fn channels(&self) -> usize {
        self.0.channels.get()
    }
}

impl PartialEq for ColorSpaceKey {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for ColorSpaceKey {}

impl Hash for ColorSpaceKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.0).hash(state);
    }
}

impl fmt::Debug for ColorSpaceKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ColorSpaceKey")
            .field("identity", &Arc::as_ptr(&self.0))
            .field("channels", &self.channels())
            .finish()
    }
}

macro_rules! predefined_space {
    ($profile:ty, $key:ident) => {
        static $key: std::sync::LazyLock<ColorSpaceKey> = std::sync::LazyLock::new(|| {
            ColorSpaceKey::new(std::num::NonZeroUsize::new(3).unwrap())
        });

        impl From<$profile> for ColorSpaceKey {
            fn from(_: $profile) -> Self {
                (*$key).clone()
            }
        }
    };
}

mod display_p3;
mod lab_d50;
mod linear_display_p3;
mod linear_srgb;
mod srgb;
mod xyz_d50;
mod xyz_d65;

pub use display_p3::DisplayP3;
pub use lab_d50::LabD50;
pub use linear_display_p3::LinearDisplayP3;
pub use linear_srgb::LinearSrgb;
pub use srgb::Srgb;
pub use xyz_d50::XyzD50;
pub use xyz_d65::XyzD65;

impl From<RgbColorSpace> for ColorSpaceKey {
    fn from(space: RgbColorSpace) -> Self {
        match space {
            RgbColorSpace::Srgb => Srgb.into(),
            RgbColorSpace::LinearSrgb => LinearSrgb.into(),
            RgbColorSpace::DisplayP3 => DisplayP3.into(),
            RgbColorSpace::LinearDisplayP3 => LinearDisplayP3.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn assert_rgb_profile<P: ColorSpace<Model = Rgb>>() {}

    #[test]
    fn predefined_spaces_have_canonical_keys() {
        assert_eq!(ColorSpaceKey::from(Srgb), ColorSpaceKey::from(Srgb));
        assert_eq!(
            ColorSpaceKey::from(Srgb),
            ColorSpaceKey::from(RgbColorSpace::Srgb)
        );
        assert_ne!(ColorSpaceKey::from(Srgb), ColorSpaceKey::from(LinearSrgb));
        assert_ne!(ColorSpaceKey::from(XyzD50), ColorSpaceKey::from(XyzD65));
    }

    #[test]
    fn cloned_custom_key_preserves_identity_without_equating_channel_counts() {
        let first = ColorSpaceKey::new(NonZeroUsize::new(4).unwrap());
        let same = first.clone();
        let separate = ColorSpaceKey::new(NonZeroUsize::new(4).unwrap());
        assert_eq!(first, same);
        assert_ne!(first, separate);
        assert_eq!(first.channels(), 4);
        let keys = HashSet::from([first]);
        assert!(keys.contains(&same));
        assert!(!keys.contains(&separate));
    }

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
