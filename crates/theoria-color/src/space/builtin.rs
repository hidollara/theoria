#[cfg(test)]
use crate::icc_profile::IccRenderingIntent;
#[cfg(test)]
use crate::model::Rgb;
use crate::transfer::{Transfer, TransferCurve, TransferError, TransferMatrix, TransferStage};

use super::{ColorProfile, ColorSpace, Pcs, SpaceConnectionError, TypedColorSpace};

macro_rules! named_space {
    ($name:ident, $model:ty) => {
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
        pub struct $name;

        impl TypedColorSpace for $name {
            type Model = $model;
        }
    };
}

fn rgb_direct_to(
    source: ColorSpace,
    counterpart: ColorSpace,
    counterpart_curve: TransferCurve,
    destination: &ColorSpace,
) -> Result<Option<Transfer>, SpaceConnectionError> {
    if source == *destination {
        return Ok(Some(Transfer::new([])?));
    }
    if counterpart != *destination {
        return Ok(None);
    }
    Ok(Some(Transfer::new([TransferStage::curves([
        counterpart_curve.clone(),
        counterpart_curve.clone(),
        counterpart_curve,
    ])?])?))
}

pub(crate) fn rgb_to_xyz_d65_transfer(
    matrix: [[f64; 3]; 3],
    curve: TransferCurve,
) -> Result<Transfer, TransferError> {
    Transfer::new([
        TransferStage::curves([curve.clone(), curve.clone(), curve])?,
        rgb_matrix(matrix)?,
    ])
}

pub(crate) fn xyz_d65_to_rgb_transfer(
    matrix: [[f64; 3]; 3],
    curve: TransferCurve,
) -> Result<Transfer, TransferError> {
    Transfer::new([
        rgb_matrix(matrix)?,
        TransferStage::curves([curve.clone(), curve.clone(), curve])?,
    ])
}

fn rgb_matrix(matrix: [[f64; 3]; 3]) -> Result<TransferStage, TransferError> {
    Ok(TransferStage::Matrix(TransferMatrix::new(
        3,
        3,
        matrix.into_iter().flatten().collect::<Vec<_>>(),
        [0.0; 3],
    )?))
}

pub(crate) fn rgb_to_pcs_transfer(transfer: Transfer) -> Result<Transfer, TransferError> {
    transfer.then(&adaptation(XyzD65::TO_XYZ_D50)?)
}

pub(crate) fn pcs_to_rgb_transfer(xyz_d65_to_rgb: Transfer) -> Result<Transfer, TransferError> {
    adaptation(XyzD65::FROM_XYZ_D50)?.then(&xyz_d65_to_rgb)
}

pub(crate) fn adaptation(matrix: [[f64; 3]; 3]) -> Result<Transfer, TransferError> {
    Transfer::new([TransferStage::Matrix(TransferMatrix::new(
        3,
        3,
        matrix.into_iter().flatten().collect::<Vec<_>>(),
        [0.0; 3],
    )?)])
}

macro_rules! builtin_space {
    ($space:ty, $definition:ident) => {
        struct $definition;

        impl From<$space> for $crate::space::ColorSpace {
            fn from(_: $space) -> Self {
                static SPACE: std::sync::LazyLock<$crate::space::ColorSpace> =
                    std::sync::LazyLock::new(|| {
                        let space = $crate::space::ColorSpace::new($definition);
                        assert_eq!(
                            space.model(),
                            <<$space as $crate::space::TypedColorSpace>::Model as $crate::model::TypedColorModel>::ERASED,
                            "typed and runtime color models differ"
                        );
                        space
                    });
                SPACE.clone()
            }
        }

        impl From<&$space> for $crate::space::ColorSpace {
            fn from(space: &$space) -> Self {
                (*space).into()
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

    fn assert_rgb_space<S: TypedColorSpace<Model = Rgb>>() {}

    #[test]
    fn builtin_spaces_have_canonical_identity() {
        assert_ne!(ColorSpace::from(Srgb), ColorSpace::from(LinearSrgb));
        assert_ne!(ColorSpace::from(XyzD50), ColorSpace::from(XyzD65));
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
            .color_space(IccRenderingIntent::RelativeColorimetric);
        let same = first.clone();
        let separate = crate::IccProfile::from_bytes(&bytes)
            .unwrap()
            .color_space(IccRenderingIntent::RelativeColorimetric);
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
