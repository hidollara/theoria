use std::{
    fmt,
    hash::{Hash, Hasher},
    num::NonZeroUsize,
    sync::Arc,
};

use super::color_transform::ColorTransform;
use super::icc::IccPcs;
use super::icc_profile::IccRenderingIntent;
use super::profile::{ColorProfile, ProfileConnectionError, require_relative};
use super::transfer::{Transfer, TransferCurve, TransferError, TransferMatrix, TransferStage};
use super::transform::multiply_f64;
use super::{ColorModel, Rgb, RgbTransfer};

/// Associates a statically typed color space with its coordinate model.
///
/// This is a compile-time constraint on color values. The runtime space
/// identifies their meaning; conversion is supplied by a profile separately.
pub trait TypedColorSpace: 'static {
    type Model: ColorModel;

    fn color_space(&self) -> ColorSpace;
}

macro_rules! named_space {
    ($name:ident, $model:ty) => {
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
        pub struct $name;

        impl TypedColorSpace for $name {
            type Model = $model;

            fn color_space(&self) -> ColorSpace {
                (*self).into()
            }
        }
    };
}

/// A predefined RGB space in which Canvas color operations can be evaluated.
///
/// This runtime value is the erased counterpart of the zero-sized color-space
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

impl TypedColorSpace for RgbColorSpace {
    type Model = Rgb;

    fn color_space(&self) -> ColorSpace {
        (*self).into()
    }
}

impl ColorProfile for RgbColorSpace {
    fn space(&self) -> ColorSpace {
        (*self).into()
    }

    fn pcs(&self) -> IccPcs {
        IccPcs::XyzD50
    }

    fn space_to_pcs(
        &self,
        intent: IccRenderingIntent,
    ) -> Result<ColorTransform, ProfileConnectionError> {
        require_relative(intent)?;
        Ok(ColorTransform::rgb_to_pcs(*self, self.pcs())?)
    }

    fn pcs_to_space(
        &self,
        intent: IccRenderingIntent,
    ) -> Result<ColorTransform, ProfileConnectionError> {
        require_relative(intent)?;
        Ok(ColorTransform::pcs_to_rgb(self.pcs(), *self)?)
    }

    fn direct_to(
        &self,
        destination: &ColorSpace,
        intent: IccRenderingIntent,
    ) -> Result<Option<ColorTransform>, ProfileConnectionError> {
        if intent != IccRenderingIntent::RelativeColorimetric {
            return Ok(None);
        }
        for other in [
            RgbColorSpace::Srgb,
            RgbColorSpace::LinearSrgb,
            RgbColorSpace::DisplayP3,
            RgbColorSpace::LinearDisplayP3,
        ] {
            let same_primaries = matches!(
                (*self, other),
                (
                    RgbColorSpace::Srgb | RgbColorSpace::LinearSrgb,
                    RgbColorSpace::Srgb | RgbColorSpace::LinearSrgb
                ) | (
                    RgbColorSpace::DisplayP3 | RgbColorSpace::LinearDisplayP3,
                    RgbColorSpace::DisplayP3 | RgbColorSpace::LinearDisplayP3
                )
            );
            if same_primaries && ColorSpace::from(other) == *destination {
                return Ok(Some(ColorTransform::between_rgb(*self, other)?));
            }
        }
        Ok(None)
    }
}

macro_rules! predefined_rgb_profile {
    ($profile:ty) => {
        impl ColorProfile for $profile {
            fn space(&self) -> ColorSpace {
                (*self).into()
            }

            fn pcs(&self) -> IccPcs {
                IccPcs::XyzD50
            }

            fn space_to_pcs(
                &self,
                intent: IccRenderingIntent,
            ) -> Result<ColorTransform, ProfileConnectionError> {
                RgbColorSpace::from(*self).space_to_pcs(intent)
            }

            fn pcs_to_space(
                &self,
                intent: IccRenderingIntent,
            ) -> Result<ColorTransform, ProfileConnectionError> {
                RgbColorSpace::from(*self).pcs_to_space(intent)
            }

            fn direct_to(
                &self,
                destination: &ColorSpace,
                intent: IccRenderingIntent,
            ) -> Result<Option<ColorTransform>, ProfileConnectionError> {
                RgbColorSpace::from(*self).direct_to(destination, intent)
            }
        }
    };
}

impl RgbColorSpace {
    /// Whether straight RGB coordinates in this space are linear-light.
    pub const fn is_linear(self) -> bool {
        matches!(self.transfer_curve(), RgbTransfer::Linear)
    }

    pub(crate) const fn transfer_curve(self) -> RgbTransfer {
        match self {
            Self::Srgb | Self::DisplayP3 => RgbTransfer::Srgb,
            Self::LinearSrgb | Self::LinearDisplayP3 => RgbTransfer::Linear,
        }
    }

    pub(crate) const fn matrices(self) -> ([[f64; 3]; 3], [[f64; 3]; 3]) {
        match self {
            Self::Srgb => (Srgb::TO_XYZ_D65, Srgb::FROM_XYZ_D65),
            Self::LinearSrgb => (LinearSrgb::TO_XYZ_D65, LinearSrgb::FROM_XYZ_D65),
            Self::DisplayP3 => (DisplayP3::TO_XYZ_D65, DisplayP3::FROM_XYZ_D65),
            Self::LinearDisplayP3 => (LinearDisplayP3::TO_XYZ_D65, LinearDisplayP3::FROM_XYZ_D65),
        }
    }

    pub(crate) fn to_xyz_d65_transfer(self) -> Result<Transfer, TransferError> {
        let curve = match self.transfer_curve() {
            RgbTransfer::Linear => TransferCurve::IDENTITY,
            RgbTransfer::Srgb => TransferCurve::SRGB_DECODE,
        };
        Transfer::new(
            3,
            [
                TransferStage::curves([curve.clone(), curve.clone(), curve])?,
                rgb_matrix(self.matrices().0)?,
            ],
        )
    }

    pub(crate) fn xyz_d65_to_rgb_transfer(self) -> Result<Transfer, TransferError> {
        let curve = match self.transfer_curve() {
            RgbTransfer::Linear => TransferCurve::IDENTITY,
            RgbTransfer::Srgb => TransferCurve::SRGB_ENCODE,
        };
        Transfer::new(
            3,
            [
                rgb_matrix(self.matrices().1)?,
                TransferStage::curves([curve.clone(), curve.clone(), curve])?,
            ],
        )
    }

    pub(crate) fn to_xyz_d65_f64(self, rgb: [f64; 3]) -> [f64; 3] {
        multiply_f64(
            self.matrices().0,
            rgb.map(|value| self.transfer_curve().decode_f64(value)),
        )
    }

    pub(crate) fn xyz_d65_to_rgb_f64(self, xyz: [f64; 3]) -> [f64; 3] {
        multiply_f64(self.matrices().1, xyz).map(|value| self.transfer_curve().encode_f64(value))
    }
}

fn rgb_matrix(matrix: [[f64; 3]; 3]) -> Result<TransferStage, TransferError> {
    Ok(TransferStage::Matrix(TransferMatrix::new(
        3,
        3,
        matrix.into_iter().flatten().collect::<Vec<_>>(),
        [0.0; 3],
    )?))
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

/// One runtime color space, identified independently of any profile data.
///
/// Clones denote the same coordinate space. Constructing another instance,
/// even with the same channel count, denotes a different space. It keeps only
/// its identity alive, not an ICC profile or its LUTs.
#[derive(Clone)]
pub struct ColorSpace(Arc<ColorSpaceIdentity>);

impl ColorSpace {
    /// Creates a distinct space with the specified number of channels.
    pub fn new(channels: NonZeroUsize) -> Self {
        Self(Arc::new(ColorSpaceIdentity { channels }))
    }

    pub fn channels(&self) -> usize {
        self.0.channels.get()
    }
}

impl PartialEq for ColorSpace {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for ColorSpace {}

impl Hash for ColorSpace {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.0).hash(state);
    }
}

impl fmt::Debug for ColorSpace {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ColorSpace")
            .field("identity", &Arc::as_ptr(&self.0))
            .field("channels", &self.channels())
            .finish()
    }
}

macro_rules! predefined_space {
    ($profile:ty, $key:ident) => {
        static $key: std::sync::LazyLock<ColorSpace> =
            std::sync::LazyLock::new(|| ColorSpace::new(std::num::NonZeroUsize::new(3).unwrap()));

        impl From<$profile> for ColorSpace {
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

impl From<RgbColorSpace> for ColorSpace {
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

    fn assert_rgb_space<P: TypedColorSpace<Model = Rgb>>() {}

    #[test]
    fn predefined_spaces_have_canonical_identity() {
        assert_eq!(ColorSpace::from(Srgb), ColorSpace::from(Srgb));
        assert_eq!(
            ColorSpace::from(Srgb),
            ColorSpace::from(RgbColorSpace::Srgb)
        );
        assert_ne!(ColorSpace::from(Srgb), ColorSpace::from(LinearSrgb));
        assert_ne!(ColorSpace::from(XyzD50), ColorSpace::from(XyzD65));
    }

    #[test]
    fn cloned_custom_space_preserves_identity_without_equating_channel_counts() {
        let first = ColorSpace::new(NonZeroUsize::new(4).unwrap());
        let same = first.clone();
        let separate = ColorSpace::new(NonZeroUsize::new(4).unwrap());
        assert_eq!(first, same);
        assert_ne!(first, separate);
        assert_eq!(first.channels(), 4);
        let keys = HashSet::from([first]);
        assert!(keys.contains(&same));
        assert!(!keys.contains(&separate));
    }

    #[test]
    fn spaces_declare_their_coordinate_model() {
        assert_rgb_space::<Srgb>();
        assert_rgb_space::<LinearSrgb>();
        assert_rgb_space::<DisplayP3>();
        assert_rgb_space::<LinearDisplayP3>();
    }

    #[test]
    fn predefined_rgb_spaces_erase_to_operation_spaces() {
        assert_eq!(RgbColorSpace::from(Srgb), RgbColorSpace::Srgb);
        assert_eq!(
            RgbColorSpace::from(LinearDisplayP3),
            RgbColorSpace::LinearDisplayP3
        );
    }

    #[test]
    fn rgb_spaces_round_trip_through_xyz_d65() {
        fn check(space: RgbColorSpace) {
            let rgb = [0.23, 0.51, 0.79];
            let xyz = space.to_xyz_d65_f64(rgb);
            let actual = space.xyz_d65_to_rgb_f64(xyz);
            for (actual, expected) in actual.into_iter().zip(rgb) {
                assert!((actual - expected).abs() < 0.000_01);
            }
        }

        check(RgbColorSpace::Srgb);
        check(RgbColorSpace::LinearSrgb);
        check(RgbColorSpace::DisplayP3);
        check(RgbColorSpace::LinearDisplayP3);
    }
}
