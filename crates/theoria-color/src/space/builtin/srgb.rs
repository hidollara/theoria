use std::sync::LazyLock;

use super::{
    ColorProfile, ColorSpace, ConnectionSpace, HslSrgb, HwbSrgb, LinearSrgb, SpaceConnectionError,
    TypedColorSpace,
};
use crate::model::Rgb;
use crate::transfer::{CurveSegment, CurveSegmentKind, Transfer, TransferCurve, TransferError};

named_space!(Srgb, Rgb);

const SRGB_ENCODE_THRESHOLD: f64 = 0.003_130_8;
const SRGB_DECODE_THRESHOLD: f64 = 0.04045;
const SRGB_GAMMA: f64 = 2.4;
const SRGB_LINEAR_SCALE: f64 = 12.92;
const SRGB_POWER_SCALE: f64 = 1.055;
const SRGB_POWER_OFFSET: f64 = 0.055;

static SRGB_DECODE_SEGMENTS: [CurveSegment; 3] = [
    CurveSegment::exclusive(
        -SRGB_DECODE_THRESHOLD,
        CurveSegmentKind::PowerAffine {
            scale: -1.0 / SRGB_POWER_SCALE,
            offset: SRGB_POWER_OFFSET / SRGB_POWER_SCALE,
            exponent: SRGB_GAMMA,
            output_scale: -1.0,
            output_offset: 0.0,
        },
    ),
    CurveSegment::new(
        SRGB_DECODE_THRESHOLD,
        CurveSegmentKind::Linear {
            scale: 1.0 / SRGB_LINEAR_SCALE,
            offset: 0.0,
        },
    ),
    CurveSegment::new(
        f64::INFINITY,
        CurveSegmentKind::PowerAffine {
            scale: 1.0 / SRGB_POWER_SCALE,
            offset: SRGB_POWER_OFFSET / SRGB_POWER_SCALE,
            exponent: SRGB_GAMMA,
            output_scale: 1.0,
            output_offset: 0.0,
        },
    ),
];

static SRGB_ENCODE_SEGMENTS: [CurveSegment; 3] = [
    CurveSegment::exclusive(
        -SRGB_ENCODE_THRESHOLD,
        CurveSegmentKind::PowerAffine {
            scale: -1.0,
            offset: 0.0,
            exponent: 1.0 / SRGB_GAMMA,
            output_scale: -SRGB_POWER_SCALE,
            output_offset: SRGB_POWER_OFFSET,
        },
    ),
    CurveSegment::new(
        SRGB_ENCODE_THRESHOLD,
        CurveSegmentKind::Linear {
            scale: SRGB_LINEAR_SCALE,
            offset: 0.0,
        },
    ),
    CurveSegment::new(
        f64::INFINITY,
        CurveSegmentKind::PowerAffine {
            scale: 1.0,
            offset: 0.0,
            exponent: 1.0 / SRGB_GAMMA,
            output_scale: SRGB_POWER_SCALE,
            output_offset: -SRGB_POWER_OFFSET,
        },
    ),
];

impl Srgb {
    /// The extended-range sRGB encoding curve, expressed as ordinary segments.
    /// Backends can compare curves with this definition to recognize the operation.
    pub fn encode_curve() -> &'static TransferCurve {
        static CURVE: LazyLock<TransferCurve> = LazyLock::new(|| {
            TransferCurve::piecewise(SRGB_ENCODE_SEGMENTS.clone())
                .expect("valid sRGB curve definition")
        });
        &CURVE
    }

    /// The extended-range sRGB decoding curve, expressed as ordinary segments.
    /// Backends can compare curves with this definition to recognize the operation.
    pub fn decode_curve() -> &'static TransferCurve {
        static CURVE: LazyLock<TransferCurve> = LazyLock::new(|| {
            TransferCurve::piecewise(SRGB_DECODE_SEGMENTS.clone())
                .expect("valid sRGB curve definition")
        });
        &CURVE
    }

    pub(crate) fn transfer_to_linear_srgb() -> Result<Transfer, TransferError> {
        Transfer::curves(vec![Self::decode_curve().clone(); 3])
    }

    pub(crate) fn transfer_from_linear_srgb() -> Result<Transfer, TransferError> {
        Transfer::curves(vec![Self::encode_curve().clone(); 3])
    }

    pub(crate) fn transfer_to_xyz_d65() -> Result<Transfer, TransferError> {
        Self::transfer_to_linear_srgb()?.then(&LinearSrgb::transfer_to_xyz_d65()?)
    }

    pub(crate) fn transfer_from_xyz_d65() -> Result<Transfer, TransferError> {
        LinearSrgb::transfer_from_xyz_d65()?.then(&Srgb::transfer_from_linear_srgb()?)
    }
}

builtin_space!(Srgb, SrgbProfile);

impl ColorProfile for SrgbProfile {
    fn model(&self) -> crate::model::ColorModel {
        <Rgb as crate::model::TypedColorModel>::MODEL
    }

    fn connection_space(&self) -> ConnectionSpace {
        ConnectionSpace::XyzD65
    }

    fn transfer_to_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(Srgb::transfer_to_xyz_d65()?)
    }

    fn transfer_from_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(Srgb::transfer_from_xyz_d65()?)
    }

    fn direct_transfer_to(
        &self,
        destination: &ColorSpace,
    ) -> Result<Option<Transfer>, SpaceConnectionError> {
        if *destination == ColorSpace::from(super::LinearSrgb) {
            Ok(Some(Srgb::transfer_to_linear_srgb()?))
        } else if *destination == ColorSpace::from(HslSrgb) {
            Ok(Some(HslSrgb::transfer_from_srgb()?))
        } else if *destination == ColorSpace::from(HwbSrgb) {
            Ok(Some(HwbSrgb::transfer_from_srgb()?))
        } else {
            Ok(None)
        }
    }
}
