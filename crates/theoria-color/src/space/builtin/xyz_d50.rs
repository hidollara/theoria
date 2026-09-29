use super::{ColorProfile, ColorSpace, Pcs, SpaceConnectionError, Transfer, TypedColorSpace};
use crate::model::Xyz;
use crate::transfer::{
    CurveSegment, CurveSegmentKind, ICC_PCS_D50_XYZ, TransferCurve, TransferError, TransferMatrix,
    TransferStage,
};

named_space!(XyzD50, Xyz);
builtin_space!(XyzD50, XyzD50Profile);

impl XyzD50 {
    pub(crate) fn to_lab_transfer() -> Result<Transfer, TransferError> {
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
        Ok(Transfer::new([])?)
    }

    fn pcs_to_space_transfer(&self) -> Result<Transfer, SpaceConnectionError> {
        self.space_to_pcs_transfer()
    }

    fn direct_transfer_to(
        &self,
        destination: &ColorSpace,
    ) -> Result<Option<Transfer>, SpaceConnectionError> {
        if *destination == ColorSpace::from(super::LabD50) {
            Ok(Some(XyzD50::to_lab_transfer()?))
        } else {
            Ok(None)
        }
    }
}
