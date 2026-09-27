use std::{error::Error, fmt};

use super::icc_profile::IccRenderingIntent;
use super::profile::ColorSpace;
use super::profile::Pcs;
use super::space::{ColorSpaceId, LabD50, XyzD50, XyzD65};
use super::transfer::{Transfer, TransferError, TransferMatrix, TransferStage};

/// A transfer together with the meanings of its input and output coordinates.
///
/// [`Transfer`] remains the low-level operation sequence. Composition through
/// this type additionally checks semantic endpoints instead of merely matching
/// channel counts. A renderer may choose a different execution plan for the
/// same transform (for example, sRGB image-format conversion instead of a
/// shader curve) without changing its logical meaning.
#[derive(Clone, Debug, PartialEq)]
pub struct ColorTransform {
    from: ColorSpaceId,
    to: ColorSpaceId,
    transfer: Transfer,
}

impl ColorTransform {
    /// Labels an operation sequence with its semantic endpoints.
    ///
    /// The caller must ensure the stages actually implement this conversion;
    /// only channel arity can be checked here. Prefer constructors that derive
    /// the stages and endpoints together when one is available.
    pub fn new(
        from: ColorSpace,
        to: ColorSpace,
        transfer: Transfer,
    ) -> Result<Self, ColorTransformError> {
        Self::from_ids(from.id(), to.id(), transfer)
    }

    pub(crate) fn from_ids(
        from: ColorSpaceId,
        to: ColorSpaceId,
        transfer: Transfer,
    ) -> Result<Self, ColorTransformError> {
        if from.channels() != transfer.input_channels() {
            return Err(ColorTransformError::Channels {
                expected: from.channels(),
                actual: transfer.input_channels(),
            });
        }
        if to.channels() != transfer.output_channels() {
            return Err(ColorTransformError::Channels {
                expected: to.channels(),
                actual: transfer.output_channels(),
            });
        }
        Ok(Self { from, to, transfer })
    }

    /// Bridges the two media-relative D50 PCS representations without changing color.
    pub fn between_pcs(from: Pcs, to: Pcs) -> Result<Self, ColorTransformError> {
        Self::between_pcs_for_intent(from, to, IccRenderingIntent::RelativeColorimetric)
    }

    /// Bridges PCS encodings while preserving their rendering-intent meaning.
    /// Absolute PCS coordinates have a distinct identity from media-relative
    /// coordinates even when both use the same XYZ or Lab representation.
    pub fn between_pcs_for_intent(
        from: Pcs,
        to: Pcs,
        intent: IccRenderingIntent,
    ) -> Result<Self, ColorTransformError> {
        let stages = match (from, to) {
            (Pcs::XyzD50, Pcs::LabD50) => vec![TransferStage::XyzD50ToLabD50],
            (Pcs::LabD50, Pcs::XyzD50) => vec![TransferStage::LabD50ToXyzD50],
            _ => Vec::new(),
        };
        Self::from_ids(
            pcs_key_for_intent(from, intent),
            pcs_key_for_intent(to, intent),
            Transfer::new(3, stages)?,
        )
    }

    #[cfg(test)]
    pub(crate) const fn from(&self) -> &ColorSpaceId {
        &self.from
    }

    #[cfg(test)]
    pub(crate) const fn to(&self) -> &ColorSpaceId {
        &self.to
    }

    pub const fn transfer(&self) -> &Transfer {
        &self.transfer
    }

    pub fn into_transfer(self) -> Transfer {
        self.transfer
    }

    pub fn then(&self, next: &Self) -> Result<Self, ColorTransformError> {
        if self.to != next.from {
            return Err(ColorTransformError::SpaceMismatch);
        }
        Self::from_ids(
            self.from.clone(),
            next.to.clone(),
            self.transfer.then(&next.transfer)?,
        )
    }
}

pub(crate) fn rgb_to_pcs_transfer(
    mut transfer: Transfer,
    to: Pcs,
) -> Result<Transfer, TransferError> {
    transfer = transfer.then(&adaptation(XyzD65::TO_XYZ_D50)?)?;
    if to == Pcs::LabD50 {
        transfer = transfer.then(&Transfer::new(3, [TransferStage::XyzD50ToLabD50])?)?;
    }
    Ok(transfer)
}

pub(crate) fn pcs_to_rgb_transfer(
    from: Pcs,
    xyz_d65_to_rgb: Transfer,
) -> Result<Transfer, TransferError> {
    let mut transfer = Transfer::new(3, Vec::<TransferStage>::new())?;
    if from == Pcs::LabD50 {
        transfer = transfer.then(&Transfer::new(3, [TransferStage::LabD50ToXyzD50])?)?;
    }
    transfer = transfer.then(&adaptation(XyzD65::FROM_XYZ_D50)?)?;
    transfer.then(&xyz_d65_to_rgb)
}

pub(crate) fn pcs_key(pcs: Pcs) -> ColorSpaceId {
    match pcs {
        Pcs::XyzD50 => XyzD50.into(),
        Pcs::LabD50 => LabD50.into(),
    }
}

pub(crate) fn pcs_key_for_intent(pcs: Pcs, intent: IccRenderingIntent) -> ColorSpaceId {
    if intent == IccRenderingIntent::AbsoluteColorimetric {
        ColorSpaceId::absolute_pcs(pcs)
    } else {
        pcs_key(pcs)
    }
}

pub(crate) fn adaptation(matrix: [[f64; 3]; 3]) -> Result<Transfer, TransferError> {
    Transfer::new(
        3,
        [TransferStage::Matrix(TransferMatrix::new(
            3,
            3,
            matrix.into_iter().flatten().collect::<Vec<_>>(),
            [0.0; 3],
        )?)],
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorTransformError {
    Channels { expected: usize, actual: usize },
    SpaceMismatch,
    Transfer(TransferError),
}

impl From<TransferError> for ColorTransformError {
    fn from(error: TransferError) -> Self {
        Self::Transfer(error)
    }
}

impl fmt::Display for ColorTransformError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Channels { expected, actual } => {
                write!(
                    formatter,
                    "color space requires {expected} channels, got {actual}"
                )
            }
            Self::SpaceMismatch => formatter.write_str("color transform spaces do not match"),
            Self::Transfer(error) => error.fmt(formatter),
        }
    }
}

impl Error for ColorTransformError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Transfer(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::space::{LabD50, LinearSrgb, Srgb, XyzD50};
    use crate::{ColorSpace, IccProfile, IccRenderingIntent, TransferCurve, connect_spaces};

    #[test]
    fn matching_channel_counts_do_not_imply_matching_spaces() {
        let identity = Transfer::new(3, Vec::<TransferStage>::new()).unwrap();
        let first = ColorTransform::new(Srgb.into(), XyzD50.into(), identity.clone()).unwrap();
        let second = ColorTransform::new(LabD50.into(), LinearSrgb.into(), identity).unwrap();
        assert_eq!(first.then(&second), Err(ColorTransformError::SpaceMismatch));
    }

    #[test]
    fn srgb_encoding_change_is_one_recognizable_stage() {
        let transform = connect_spaces(
            Srgb,
            LinearSrgb,
            IccRenderingIntent::RelativeColorimetric,
            IccRenderingIntent::RelativeColorimetric,
        )
        .unwrap();
        assert_eq!(
            transform.transfer().stages(),
            &[TransferStage::curves([TransferCurve::SRGB_DECODE; 3]).unwrap()]
        );
        let expected = crate::transform::decode_srgb_f64(0.5);
        let actual = transform.transfer().evaluate(&[0.5; 3]).unwrap();
        for value in actual {
            assert!((value - expected).abs() < 1e-12);
        }
    }

    #[test]
    fn builtin_rgb_connects_to_icc_profile_through_pcs() {
        let bytes = moxcms::ColorProfile::new_srgb().encode().unwrap();
        let profile = IccProfile::from_bytes(&bytes).unwrap();
        let source = ColorSpace::from(Srgb)
            .space_to_pcs(IccRenderingIntent::RelativeColorimetric)
            .unwrap();
        let destination = profile
            .pcs_to_space(IccRenderingIntent::RelativeColorimetric)
            .unwrap();
        let connected = source.then(&destination).unwrap();
        let sample = [0.3, 0.5, 0.7];
        let result = connected.transfer().evaluate(&sample).unwrap();
        for (actual, expected) in result.into_iter().zip(sample) {
            assert!((actual - expected).abs() < 0.004);
        }
        assert_eq!(connected.from(), &ColorSpaceId::from(Srgb));
        assert_eq!(connected.to(), &profile.color_space().id());

        let reverse = profile
            .space_to_pcs(IccRenderingIntent::RelativeColorimetric)
            .unwrap()
            .then(
                &ColorSpace::from(Srgb)
                    .pcs_to_space(IccRenderingIntent::RelativeColorimetric)
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(reverse.to(), &ColorSpaceId::from(Srgb));
    }

    #[test]
    fn pcs_bridge_joins_transforms_with_different_pcs() {
        let xyz = ColorSpace::from(LinearSrgb)
            .space_to_pcs(IccRenderingIntent::RelativeColorimetric)
            .unwrap();
        let bridge = ColorTransform::between_pcs(Pcs::XyzD50, Pcs::LabD50).unwrap();
        let lab = xyz.then(&bridge).unwrap();
        assert_eq!(lab.to(), &ColorSpaceId::from(LabD50));
        let reverse = ColorTransform::between_pcs(Pcs::LabD50, Pcs::XyzD50)
            .unwrap()
            .then(
                &ColorSpace::from(LinearSrgb)
                    .pcs_to_space(IccRenderingIntent::RelativeColorimetric)
                    .unwrap(),
            )
            .unwrap();
        let identity = lab.then(&reverse).unwrap();
        let sample = [0.2, 0.4, 0.6];
        for (actual, expected) in identity
            .transfer()
            .evaluate(&sample)
            .unwrap()
            .into_iter()
            .zip(sample)
        {
            assert!((actual - expected).abs() < 1e-7);
        }
    }
}
