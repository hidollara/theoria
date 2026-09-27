use std::{
    fmt,
    hash::{Hash, Hasher},
    num::NonZeroUsize,
    sync::Arc,
};

use super::ColorModel;
#[cfg(test)]
use super::Rgb;
use super::color_transform::ColorTransform;
use super::icc_profile::IccRenderingIntent;
use super::profile::{
    ColorProfile, Pcs, ProfileConnectionError, ProfileDefinition, require_relative,
};
use super::transfer::{Transfer, TransferCurve, TransferError, TransferMatrix, TransferStage};

/// Associates a statically typed color space with its coordinate model.
///
/// This is a compile-time constraint on color values. The runtime space
/// identifies their meaning; conversion is supplied by a profile separately.
/// Named spaces convert to one shared standard profile rather than defining
/// a new profile themselves.
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

fn rgb_direct_to(
    source: ColorProfile,
    counterpart: ColorProfile,
    counterpart_curve: TransferCurve,
    destination: &ColorProfile,
    intent: IccRenderingIntent,
) -> Result<Option<ColorTransform>, ProfileConnectionError> {
    if intent != IccRenderingIntent::RelativeColorimetric {
        return Ok(None);
    }
    if source == *destination {
        return Ok(Some(ColorTransform::new(
            source.space(),
            destination.space(),
            Transfer::new(3, Vec::<TransferStage>::new())?,
        )?));
    }
    if counterpart != *destination {
        return Ok(None);
    }
    Ok(Some(ColorTransform::new(
        source.space(),
        destination.space(),
        Transfer::new(
            3,
            [TransferStage::curves([
                counterpart_curve.clone(),
                counterpart_curve.clone(),
                counterpart_curve,
            ])?],
        )?,
    )?))
}

pub(crate) fn rgb_to_xyz_d65_transfer(
    matrix: [[f64; 3]; 3],
    curve: TransferCurve,
) -> Result<Transfer, TransferError> {
    Transfer::new(
        3,
        [
            TransferStage::curves([curve.clone(), curve.clone(), curve])?,
            rgb_matrix(matrix)?,
        ],
    )
}

pub(crate) fn xyz_d65_to_rgb_transfer(
    matrix: [[f64; 3]; 3],
    curve: TransferCurve,
) -> Result<Transfer, TransferError> {
    Transfer::new(
        3,
        [
            rgb_matrix(matrix)?,
            TransferStage::curves([curve.clone(), curve.clone(), curve])?,
        ],
    )
}

fn rgb_matrix(matrix: [[f64; 3]; 3]) -> Result<TransferStage, TransferError> {
    Ok(TransferStage::Matrix(TransferMatrix::new(
        3,
        3,
        matrix.into_iter().flatten().collect::<Vec<_>>(),
        [0.0; 3],
    )?))
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
    ($space:ty, $definition:ident, $key:ident) => {
        struct $definition;

        static $key: std::sync::LazyLock<ColorSpace> =
            std::sync::LazyLock::new(|| ColorSpace::new(std::num::NonZeroUsize::new(3).unwrap()));

        impl From<$space> for ColorSpace {
            fn from(_: $space) -> Self {
                (*$key).clone()
            }
        }

        impl From<$space> for ColorProfile {
            fn from(_: $space) -> Self {
                static PROFILE: std::sync::LazyLock<ColorProfile> =
                    std::sync::LazyLock::new(|| ColorProfile::new($definition));
                PROFILE.clone()
            }
        }

        impl From<&$space> for ColorProfile {
            fn from(profile: &$space) -> Self {
                (*profile).into()
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn assert_rgb_space<P: TypedColorSpace<Model = Rgb>>() {}

    #[test]
    fn predefined_spaces_have_canonical_identity() {
        assert_eq!(ColorSpace::from(Srgb), ColorSpace::from(Srgb));
        assert_ne!(ColorSpace::from(Srgb), ColorSpace::from(LinearSrgb));
        assert_ne!(ColorSpace::from(XyzD50), ColorSpace::from(XyzD65));
        assert_eq!(ColorProfile::from(Srgb), ColorProfile::from(Srgb));
        assert_eq!(
            ColorProfile::from(LinearSrgb),
            ColorProfile::from(LinearSrgb)
        );
        assert_eq!(ColorProfile::from(DisplayP3), ColorProfile::from(DisplayP3));
        assert_eq!(
            ColorProfile::from(LinearDisplayP3),
            ColorProfile::from(LinearDisplayP3)
        );
        assert_eq!(ColorProfile::from(LabD50), ColorProfile::from(LabD50));
        assert_eq!(ColorProfile::from(XyzD50), ColorProfile::from(XyzD50));
        assert_eq!(ColorProfile::from(XyzD65), ColorProfile::from(XyzD65));
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
}
