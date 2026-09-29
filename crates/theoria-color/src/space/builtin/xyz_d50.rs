use std::sync::LazyLock;

use super::{ColorProfile, ColorSpace, Pcs, SpaceConnectionError, TypedColorSpace};
use crate::model::Xyz;
use crate::transfer::Transfer;
use crate::transfer::{
    CurveSegment, CurveSegmentKind, ICC_PCS_D50_XYZ, TransferCurve, TransferError, TransferMatrix,
    TransferStage,
};

named_space!(XyzD50, Xyz);
builtin_space!(XyzD50, XyzD50Profile);

impl XyzD50 {
    // Linear Bradford adaptation from the CSS Color 4 conversion sample.
    // https://www.w3.org/TR/css-color-4/#color-conversion-code
    const TO_XYZ_D65: [[f64; 3]; 3] = [
        [0.955473421488075, -0.02309845494876471, 0.06325924320057072],
        [
            -0.0283697093338637,
            1.0099953980813041,
            0.021041441191917323,
        ],
        [
            0.012314014864481998,
            -0.020507649298898964,
            1.330365926242124,
        ],
    ];

    pub(crate) fn to_xyz_d65_transfer() -> Transfer {
        static TRANSFER: LazyLock<Transfer> = LazyLock::new(|| {
            XyzD50::build_to_xyz_d65_transfer().expect("built-in D50 to D65 adaptation is valid")
        });
        TRANSFER.clone()
    }

    fn build_to_xyz_d65_transfer() -> Result<Transfer, TransferError> {
        Transfer::new([TransferStage::Matrix(TransferMatrix::new(
            3,
            3,
            Self::TO_XYZ_D65.into_iter().flatten().collect::<Vec<_>>(),
            [0.0; 3],
        )?)])
    }

    pub(crate) fn to_lab_transfer() -> Transfer {
        static TRANSFER: LazyLock<Transfer> = LazyLock::new(|| {
            XyzD50::build_to_lab_transfer().expect("built-in XYZ D50 to Lab D50 transfer is valid")
        });
        TRANSFER.clone()
    }

    fn build_to_lab_transfer() -> Result<Transfer, TransferError> {
        const DELTA: f64 = 6.0 / 29.0;
        const EPSILON: f64 = DELTA * DELTA * DELTA;
        const LINEAR_SCALE: f64 = 3.0 * DELTA * DELTA;

        let curve = TransferCurve::piecewise([
            CurveSegment::new(
                EPSILON,
                CurveSegmentKind::Linear {
                    scale: 1.0 / LINEAR_SCALE,
                    offset: 4.0 / 29.0,
                },
            ),
            CurveSegment::new(
                f64::INFINITY,
                CurveSegmentKind::PowerAffine {
                    scale: 1.0,
                    offset: 0.0,
                    exponent: 1.0 / 3.0,
                    output_scale: 1.0,
                    output_offset: 0.0,
                },
            ),
        ])?;
        Transfer::new([
            TransferStage::Matrix(TransferMatrix::new(
                3,
                3,
                [
                    1.0 / ICC_PCS_D50_XYZ[0],
                    0.0,
                    0.0,
                    0.0,
                    1.0 / ICC_PCS_D50_XYZ[1],
                    0.0,
                    0.0,
                    0.0,
                    1.0 / ICC_PCS_D50_XYZ[2],
                ],
                [0.0; 3],
            )?),
            TransferStage::curves(vec![curve; 3])?,
            TransferStage::Matrix(TransferMatrix::new(
                3,
                3,
                [0.0, 116.0, 0.0, 500.0, -500.0, 0.0, 0.0, 200.0, -200.0],
                [-16.0, 0.0, 0.0],
            )?),
        ])
    }
}

impl ColorProfile for XyzD50Profile {
    fn model(&self) -> crate::model::ColorModel {
        crate::model::ColorModel::Xyz
    }

    fn pcs(&self) -> Pcs {
        Pcs::XyzD50
    }

    fn space_to_pcs_transfer(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(Transfer::IDENTITY)
    }

    fn pcs_to_space_transfer(&self) -> Result<Transfer, SpaceConnectionError> {
        self.space_to_pcs_transfer()
    }

    fn direct_transfer_to(
        &self,
        destination: &ColorSpace,
    ) -> Result<Option<Transfer>, SpaceConnectionError> {
        if *destination == ColorSpace::from(super::LabD50) {
            Ok(Some(XyzD50::to_lab_transfer()))
        } else if *destination == ColorSpace::from(super::XyzD65) {
            Ok(Some(XyzD50::to_xyz_d65_transfer()))
        } else {
            Ok(None)
        }
    }
}
