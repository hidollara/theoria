use std::sync::LazyLock;

use super::{
    ColorProfile, ColorSpace, ConnectionSpace, HslSrgb, HwbSrgb, SpaceConnectionError, Srgb,
    TypedColorSpace,
};
use crate::model::Rgb;
use crate::transfer::{CurveSegment, CurveSegmentKind, Transfer, TransferCurve, TransferError};

named_space!(LinearSrgb, Rgb);

const SRGB_ENCODE_THRESHOLD: f64 = 0.003_130_8;
const SRGB_LINEAR_SCALE: f64 = 12.92;
const SRGB_POWER_SCALE: f64 = 1.055;
const SRGB_POWER_OFFSET: f64 = 0.055;
const SRGB_GAMMA: f64 = 2.4;

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

impl LinearSrgb {
    /// The extended-range sRGB encoding curve, expressed as ordinary segments.
    /// Backends can compare curves with this definition to recognize the operation.
    pub fn encode_curve() -> &'static TransferCurve {
        static CURVE: LazyLock<TransferCurve> = LazyLock::new(|| {
            TransferCurve::piecewise(SRGB_ENCODE_SEGMENTS.clone())
                .expect("valid sRGB curve definition")
        });
        &CURVE
    }

    pub(crate) fn transfer_to_srgb() -> Result<Transfer, TransferError> {
        Transfer::curves(vec![Self::encode_curve().clone(); 3])
    }

    // Exact rational matrix from CSS Color 4's color-conversion sample code.
    const TO_XYZ_D65: [[f64; 3]; 3] = [
        [506752.0 / 1228815.0, 87881.0 / 245763.0, 12673.0 / 70218.0],
        [87098.0 / 409605.0, 175762.0 / 245763.0, 12673.0 / 175545.0],
        [7918.0 / 409605.0, 87881.0 / 737289.0, 1001167.0 / 1053270.0],
    ];

    // Exact rational inverse matrix from CSS Color 4's color-conversion sample code.
    const FROM_XYZ_D65: [[f64; 3]; 3] = [
        [12831.0 / 3959.0, -329.0 / 214.0, -1974.0 / 3959.0],
        [
            -851781.0 / 878810.0,
            1648619.0 / 878810.0,
            36519.0 / 878810.0,
        ],
        [705.0 / 12673.0, -2585.0 / 12673.0, 705.0 / 667.0],
    ];

    pub(crate) fn transfer_to_xyz_d65() -> Result<Transfer, TransferError> {
        Transfer::matrix(Self::TO_XYZ_D65, [0.0; 3])
    }

    pub(crate) fn transfer_from_xyz_d65() -> Result<Transfer, TransferError> {
        Transfer::matrix(Self::FROM_XYZ_D65, [0.0; 3])
    }
}

builtin_space!(LinearSrgb, LinearSrgbProfile);

impl ColorProfile for LinearSrgbProfile {
    fn model(&self) -> crate::model::ColorModel {
        <Rgb as crate::model::TypedColorModel>::MODEL
    }

    fn connection_space(&self) -> ConnectionSpace {
        ConnectionSpace::XyzD65
    }

    fn transfer_to_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(LinearSrgb::transfer_to_xyz_d65()?)
    }

    fn transfer_from_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(LinearSrgb::transfer_from_xyz_d65()?)
    }

    fn direct_transfer_to(
        &self,
        destination: &ColorSpace,
    ) -> Result<Option<Transfer>, SpaceConnectionError> {
        if *destination == ColorSpace::from(Srgb) {
            Ok(Some(LinearSrgb::transfer_to_srgb()?))
        } else if *destination == ColorSpace::from(HslSrgb) {
            Ok(Some(
                LinearSrgb::transfer_to_srgb()?.then(&HslSrgb::transfer_from_srgb()?)?,
            ))
        } else if *destination == ColorSpace::from(HwbSrgb) {
            Ok(Some(
                LinearSrgb::transfer_to_srgb()?.then(&HwbSrgb::transfer_from_srgb()?)?,
            ))
        } else {
            Ok(None)
        }
    }
}
