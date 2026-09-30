use super::{ColorProfile, ConnectionSpace, SpaceConnectionError, TypedColorSpace};
use crate::model::Rgb;
use crate::transfer::{
    CurveSegment, CurveSegmentKind, Transfer, TransferCurve, TransferError, TransferMatrix,
    TransferStage,
};

named_space!(ProPhotoRgb, Rgb);

impl ProPhotoRgb {
    const LINEAR_RGB_TO_XYZ_D50: [[f64; 3]; 3] = [
        [0.7977666449006423, 0.13518129740053308, 0.0313477341283922],
        [0.2880748288194013, 0.711835234241873, 0.00008993693872564],
        [0.0, 0.0, 0.8251046025104602],
    ];

    const XYZ_D50_TO_LINEAR_RGB: [[f64; 3]; 3] = [
        [
            1.3457868816471583,
            -0.25557208737979464,
            -0.05110186497554526,
        ],
        [-0.5446307051249019, 1.5082477428451468, 0.02052744743642139],
        [0.0, 0.0, 1.2119675456389452],
    ];

    pub(crate) fn decode_curve() -> Result<TransferCurve, TransferError> {
        let power = |negative| CurveSegmentKind::PowerAffine {
            scale: if negative { -1.0 } else { 1.0 },
            offset: 0.0,
            exponent: 1.8,
            output_scale: if negative { -1.0 } else { 1.0 },
            output_offset: 0.0,
        };
        TransferCurve::piecewise([
            CurveSegment::exclusive(-1.0 / 32.0, power(true)),
            CurveSegment::new(
                1.0 / 32.0,
                CurveSegmentKind::Linear {
                    scale: 1.0 / 16.0,
                    offset: 0.0,
                },
            ),
            CurveSegment::new(f64::INFINITY, power(false)),
        ])
    }

    pub(crate) fn encode_curve() -> Result<TransferCurve, TransferError> {
        let power = |negative| CurveSegmentKind::PowerAffine {
            scale: if negative { -1.0 } else { 1.0 },
            offset: 0.0,
            exponent: 1.0 / 1.8,
            output_scale: if negative { -1.0 } else { 1.0 },
            output_offset: 0.0,
        };
        TransferCurve::piecewise([
            CurveSegment::exclusive(-1.0 / 512.0, power(true)),
            CurveSegment::exclusive(
                1.0 / 512.0,
                CurveSegmentKind::Linear {
                    scale: 16.0,
                    offset: 0.0,
                },
            ),
            CurveSegment::new(f64::INFINITY, power(false)),
        ])
    }

    pub(crate) fn transfer_to_xyz_d50() -> Result<Transfer, TransferError> {
        let decode = TransferStage::curves(vec![Self::decode_curve()?; 3])?;
        let matrix =
            TransferStage::Matrix(TransferMatrix::new(Self::LINEAR_RGB_TO_XYZ_D50, [0.0; 3])?);
        Transfer::new([decode, matrix])
    }

    pub(crate) fn transfer_from_xyz_d50() -> Result<Transfer, TransferError> {
        Transfer::new([
            TransferStage::Matrix(TransferMatrix::new(Self::XYZ_D50_TO_LINEAR_RGB, [0.0; 3])?),
            TransferStage::curves(vec![Self::encode_curve()?; 3])?,
        ])
    }
}

builtin_space!(ProPhotoRgb, ProPhotoRgbProfile);

impl ColorProfile for ProPhotoRgbProfile {
    fn channels(&self) -> usize {
        3
    }

    fn connection_space(&self) -> ConnectionSpace {
        ConnectionSpace::XyzD50
    }

    fn transfer_to_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(ProPhotoRgb::transfer_to_xyz_d50()?)
    }

    fn transfer_from_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(ProPhotoRgb::transfer_from_xyz_d50()?)
    }
}
