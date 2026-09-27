use std::{error::Error, fmt, num::NonZeroUsize, sync::LazyLock};

use super::icc::IccPcs;
use super::icc_profile::IccRenderingIntent;
use super::space::{ColorSpace, LabD50, RgbColorSpace, XyzD50, XyzD65};
use super::transfer::{Transfer, TransferCurve, TransferError, TransferMatrix, TransferStage};

/// A transfer together with the meanings of its input and output coordinates.
///
/// [`Transfer`] remains the low-level operation sequence. Composition through
/// this type additionally checks semantic endpoints instead of merely matching
/// channel counts. A renderer may choose a different execution plan for the
/// same transform (for example, sRGB image-format conversion instead of a
/// shader curve) without changing its logical meaning.
#[derive(Clone, Debug, PartialEq)]
pub struct ColorTransform {
    from: ColorSpace,
    to: ColorSpace,
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

    /// Converts between predefined RGB spaces without an ICC profile.
    ///
    /// When only the sRGB transfer function differs, this leaves a single
    /// recognizable curve stage for a renderer to lower at an image-format
    /// boundary. Changes of primaries go through XYZ D65 instead.
    pub fn between_rgb(
        from: RgbColorSpace,
        to: RgbColorSpace,
    ) -> Result<Self, ColorTransformError> {
        let same_primaries = matches!(
            (from, to),
            (
                RgbColorSpace::Srgb | RgbColorSpace::LinearSrgb,
                RgbColorSpace::Srgb | RgbColorSpace::LinearSrgb
            ) | (
                RgbColorSpace::DisplayP3 | RgbColorSpace::LinearDisplayP3,
                RgbColorSpace::DisplayP3 | RgbColorSpace::LinearDisplayP3
            )
        );
        let transfer = if from == to {
            Transfer::new(3, Vec::<TransferStage>::new())?
        } else if same_primaries {
            let curve = if from.is_linear() {
                TransferCurve::SRGB_ENCODE
            } else {
                TransferCurve::SRGB_DECODE
            };
            Transfer::new(
                3,
                [TransferStage::curves([
                    curve.clone(),
                    curve.clone(),
                    curve,
                ])?],
            )?
        } else {
            from.to_xyz_d65_transfer()?
                .then(&to.xyz_d65_to_rgb_transfer()?)?
        };
        Self::new(from.into(), to.into(), transfer)
    }

    /// Converts predefined RGB coordinates to the media-relative D50 XYZ or Lab PCS.
    ///
    /// The RGB primary matrix is followed by D65-to-D50 adaptation. The PCS
    /// endpoint uses the same canonical identity as a loaded ICC profile.
    pub fn rgb_to_pcs(from: RgbColorSpace, to: IccPcs) -> Result<Self, ColorTransformError> {
        let mut transfer = from.to_xyz_d65_transfer()?;
        transfer = transfer.then(&adaptation(XyzD65::TO_XYZ_D50)?)?;
        if to == IccPcs::LabD50 {
            transfer = transfer.then(&Transfer::new(3, [TransferStage::XyzD50ToLabD50])?)?;
        }
        Self::new(from.into(), pcs_key(to), transfer)
    }

    /// Converts a media-relative D50 XYZ or Lab PCS to predefined RGB coordinates.
    pub fn pcs_to_rgb(from: IccPcs, to: RgbColorSpace) -> Result<Self, ColorTransformError> {
        let mut transfer = Transfer::new(3, Vec::<TransferStage>::new())?;
        if from == IccPcs::LabD50 {
            transfer = transfer.then(&Transfer::new(3, [TransferStage::LabD50ToXyzD50])?)?;
        }
        transfer = transfer.then(&adaptation(XyzD65::FROM_XYZ_D50)?)?;
        transfer = transfer.then(&to.xyz_d65_to_rgb_transfer()?)?;
        Self::new(pcs_key(from), to.into(), transfer)
    }

    /// Bridges the two media-relative D50 PCS representations without changing color.
    pub fn between_pcs(from: IccPcs, to: IccPcs) -> Result<Self, ColorTransformError> {
        Self::between_pcs_for_intent(from, to, IccRenderingIntent::RelativeColorimetric)
    }

    /// Bridges PCS encodings while preserving their rendering-intent meaning.
    /// Absolute PCS coordinates have a distinct identity from media-relative
    /// coordinates even when both use the same XYZ or Lab representation.
    pub fn between_pcs_for_intent(
        from: IccPcs,
        to: IccPcs,
        intent: IccRenderingIntent,
    ) -> Result<Self, ColorTransformError> {
        let stages = match (from, to) {
            (IccPcs::XyzD50, IccPcs::LabD50) => vec![TransferStage::XyzD50ToLabD50],
            (IccPcs::LabD50, IccPcs::XyzD50) => vec![TransferStage::LabD50ToXyzD50],
            _ => Vec::new(),
        };
        Self::new(
            pcs_key_for_intent(from, intent),
            pcs_key_for_intent(to, intent),
            Transfer::new(3, stages)?,
        )
    }

    pub const fn from(&self) -> &ColorSpace {
        &self.from
    }

    pub const fn to(&self) -> &ColorSpace {
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
        Self::new(
            self.from.clone(),
            next.to.clone(),
            self.transfer.then(&next.transfer)?,
        )
    }
}

pub(crate) fn pcs_key(pcs: IccPcs) -> ColorSpace {
    match pcs {
        IccPcs::XyzD50 => XyzD50.into(),
        IccPcs::LabD50 => LabD50.into(),
    }
}

static ABSOLUTE_XYZ_D50_KEY: LazyLock<ColorSpace> =
    LazyLock::new(|| ColorSpace::new(NonZeroUsize::new(3).unwrap()));
static ABSOLUTE_LAB_D50_KEY: LazyLock<ColorSpace> =
    LazyLock::new(|| ColorSpace::new(NonZeroUsize::new(3).unwrap()));

pub(crate) fn pcs_key_for_intent(pcs: IccPcs, intent: IccRenderingIntent) -> ColorSpace {
    if intent == IccRenderingIntent::AbsoluteColorimetric {
        match pcs {
            IccPcs::XyzD50 => (*ABSOLUTE_XYZ_D50_KEY).clone(),
            IccPcs::LabD50 => (*ABSOLUTE_LAB_D50_KEY).clone(),
        }
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
    use crate::{IccProfile, IccRenderingIntent};

    #[test]
    fn matching_channel_counts_do_not_imply_matching_spaces() {
        let identity = Transfer::new(3, Vec::<TransferStage>::new()).unwrap();
        let first = ColorTransform::new(Srgb.into(), XyzD50.into(), identity.clone()).unwrap();
        let second = ColorTransform::new(LabD50.into(), LinearSrgb.into(), identity).unwrap();
        assert_eq!(first.then(&second), Err(ColorTransformError::SpaceMismatch));
    }

    #[test]
    fn srgb_encoding_change_is_one_recognizable_stage() {
        let transform =
            ColorTransform::between_rgb(RgbColorSpace::Srgb, RgbColorSpace::LinearSrgb).unwrap();
        assert_eq!(
            transform.transfer().stages(),
            &[TransferStage::curves([TransferCurve::SRGB_DECODE; 3]).unwrap()]
        );
        let expected = crate::transform::RgbTransfer::Srgb.decode_f64(0.5);
        let actual = transform.transfer().evaluate(&[0.5; 3]).unwrap();
        for value in actual {
            assert!((value - expected).abs() < 1e-12);
        }
    }

    #[test]
    fn builtin_rgb_connects_to_icc_profile_through_pcs() {
        let bytes = moxcms::ColorProfile::new_srgb().encode().unwrap();
        let profile = IccProfile::from_bytes(&bytes).unwrap();
        let source = ColorTransform::rgb_to_pcs(RgbColorSpace::Srgb, IccPcs::XyzD50).unwrap();
        let destination = profile
            .pcs_to_space(IccRenderingIntent::RelativeColorimetric)
            .unwrap();
        let connected = source.then(&destination).unwrap();
        let sample = [0.3, 0.5, 0.7];
        let result = connected.transfer().evaluate(&sample).unwrap();
        for (actual, expected) in result.into_iter().zip(sample) {
            assert!((actual - expected).abs() < 0.004);
        }
        assert_eq!(connected.from(), &ColorSpace::from(Srgb));
        assert_eq!(connected.to(), &profile.device_space());

        let reverse = profile
            .space_to_pcs(IccRenderingIntent::RelativeColorimetric)
            .unwrap()
            .then(&ColorTransform::pcs_to_rgb(IccPcs::XyzD50, RgbColorSpace::Srgb).unwrap())
            .unwrap();
        assert_eq!(reverse.to(), &ColorSpace::from(Srgb));
    }

    #[test]
    fn pcs_bridge_joins_transforms_with_different_pcs() {
        let xyz = ColorTransform::rgb_to_pcs(RgbColorSpace::LinearSrgb, IccPcs::XyzD50).unwrap();
        let bridge = ColorTransform::between_pcs(IccPcs::XyzD50, IccPcs::LabD50).unwrap();
        let lab = xyz.then(&bridge).unwrap();
        assert_eq!(lab.to(), &ColorSpace::from(LabD50));
        let reverse =
            ColorTransform::pcs_to_rgb(IccPcs::LabD50, RgbColorSpace::LinearSrgb).unwrap();
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
