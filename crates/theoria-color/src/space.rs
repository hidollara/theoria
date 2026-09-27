use std::{
    fmt,
    hash::{Hash, Hasher},
    sync::Weak,
};

use super::ColorModel;
#[cfg(test)]
use super::Rgb;
use super::color_transform::{pcs_to_rgb_transfer, rgb_to_pcs_transfer};
use super::icc_profile::IccRenderingIntent;
use super::profile::{
    ColorSpace, Pcs, ProfileConnectionError, ProfileDefinition, require_relative,
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

            fn color_space(&self) -> $crate::profile::ColorSpace {
                (*self).into()
            }
        }
    };
}

fn rgb_direct_to(
    source: ColorSpace,
    counterpart: ColorSpace,
    counterpart_curve: TransferCurve,
    destination: &ColorSpace,
    intent: IccRenderingIntent,
) -> Result<Option<Transfer>, ProfileConnectionError> {
    if intent != IccRenderingIntent::RelativeColorimetric {
        return Ok(None);
    }
    if source == *destination {
        return Ok(Some(Transfer::new(3, Vec::<TransferStage>::new())?));
    }
    if counterpart != *destination {
        return Ok(None);
    }
    Ok(Some(Transfer::new(
        3,
        [TransferStage::curves([
            counterpart_curve.clone(),
            counterpart_curve.clone(),
            counterpart_curve,
        ])?],
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

/// A transform endpoint that does not retain a profile's conversion data.
/// Keeping a weak reference also prevents an allocation address from being
/// reused while a transform still refers to that endpoint.
#[derive(Clone)]
pub(crate) enum ColorSpaceId {
    Space(Weak<dyn ProfileDefinition>, usize),
    AbsolutePcs(Pcs),
}

impl ColorSpaceId {
    pub(crate) fn of(definition: Weak<dyn ProfileDefinition>, channels: usize) -> Self {
        Self::Space(definition, channels)
    }

    pub(crate) fn absolute_pcs(pcs: Pcs) -> Self {
        Self::AbsolutePcs(pcs)
    }

    pub fn channels(&self) -> usize {
        match self {
            Self::Space(_, channels) => *channels,
            Self::AbsolutePcs(_) => 3,
        }
    }
}

impl PartialEq for ColorSpaceId {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Space(left, _), Self::Space(right, _)) => Weak::ptr_eq(left, right),
            (Self::AbsolutePcs(left), Self::AbsolutePcs(right)) => left == right,
            _ => false,
        }
    }
}

impl Eq for ColorSpaceId {}

impl Hash for ColorSpaceId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Self::Space(definition, _) => {
                0_u8.hash(state);
                (definition.as_ptr() as *const ()).hash(state);
            }
            Self::AbsolutePcs(pcs) => {
                1_u8.hash(state);
                pcs.hash(state);
            }
        }
    }
}

impl fmt::Debug for ColorSpaceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Space(definition, _) => formatter
                .debug_struct("ColorSpaceId")
                .field("identity", &(definition.as_ptr() as *const ()))
                .field("channels", &self.channels())
                .finish(),
            Self::AbsolutePcs(pcs) => formatter.debug_tuple("AbsolutePcs").field(pcs).finish(),
        }
    }
}

macro_rules! predefined_space {
    ($space:ty, $definition:ident) => {
        struct $definition;

        impl From<$space> for $crate::space::ColorSpaceId {
            fn from(_: $space) -> Self {
                $crate::profile::ColorSpace::from(<$space>::default()).id()
            }
        }

        impl From<$space> for $crate::profile::ColorSpace {
            fn from(_: $space) -> Self {
                static PROFILE: std::sync::LazyLock<$crate::profile::ColorSpace> =
                    std::sync::LazyLock::new(|| $crate::profile::ColorSpace::new($definition));
                PROFILE.clone()
            }
        }

        impl From<&$space> for $crate::profile::ColorSpace {
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
        assert_eq!(ColorSpaceId::from(Srgb), ColorSpaceId::from(Srgb));
        assert_ne!(ColorSpaceId::from(Srgb), ColorSpaceId::from(LinearSrgb));
        assert_ne!(ColorSpaceId::from(XyzD50), ColorSpaceId::from(XyzD65));
        assert_eq!(ColorSpace::from(Srgb), ColorSpace::from(Srgb));
        assert_eq!(ColorSpace::from(LinearSrgb), ColorSpace::from(LinearSrgb));
        assert_eq!(ColorSpace::from(DisplayP3), ColorSpace::from(DisplayP3));
        assert_eq!(
            ColorSpace::from(LinearDisplayP3),
            ColorSpace::from(LinearDisplayP3)
        );
        assert_eq!(ColorSpace::from(LabD50), ColorSpace::from(LabD50));
        assert_eq!(ColorSpace::from(XyzD50), ColorSpace::from(XyzD50));
        assert_eq!(ColorSpace::from(XyzD65), ColorSpace::from(XyzD65));
    }

    #[test]
    fn loaded_spaces_preserve_identity_without_equating_channel_counts() {
        let bytes = moxcms::ColorProfile::new_srgb().encode().unwrap();
        let first = crate::IccProfile::from_bytes(&bytes)
            .unwrap()
            .device_space()
            .id();
        let same = first.clone();
        let separate = crate::IccProfile::from_bytes(&bytes)
            .unwrap()
            .device_space()
            .id();
        assert_eq!(first, same);
        assert_ne!(first, separate);
        assert_eq!(first.channels(), 3);
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
