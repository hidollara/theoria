use std::sync::LazyLock;

use super::{
    ColorProfile, ColorSpace, ConnectionSpace, LchD50, SpaceConnectionError, TypedColorSpace,
};
use crate::model::Lab;
use crate::transfer::Transfer;
use crate::transfer::{
    CurveSegment, CurveSegmentKind, ICC_PCS_D50_XYZ, TransferCurve, TransferError,
};

named_space!(LabD50, Lab);
builtin_space!(LabD50, LabD50Profile);

impl LabD50 {
    // Maps Lab coordinates to f(X/Xn), f(Y/Yn), f(Z/Zn).
    const LAB_TO_F_XYZ: [[f64; 3]; 3] = [
        [1.0 / 116.0, 1.0 / 500.0, 0.0],
        [1.0 / 116.0, 0.0, 0.0],
        [1.0 / 116.0, 0.0, -1.0 / 200.0],
    ];

    const NORMALIZED_XYZ_TO_XYZ_D50: [[f64; 3]; 3] = [
        [ICC_PCS_D50_XYZ[0], 0.0, 0.0],
        [0.0, ICC_PCS_D50_XYZ[1], 0.0],
        [0.0, 0.0, ICC_PCS_D50_XYZ[2]],
    ];

    pub(crate) fn transfer_to_xyz_d50() -> Transfer {
        static TRANSFER: LazyLock<Transfer> = LazyLock::new(|| {
            LabD50::build_transfer_to_xyz_d50()
                .expect("built-in Lab D50 to XYZ D50 transfer is valid")
        });
        TRANSFER.clone()
    }

    fn build_transfer_to_xyz_d50() -> Result<Transfer, TransferError> {
        const DELTA: f64 = 6.0 / 29.0;
        const LINEAR_SCALE: f64 = 3.0 * DELTA * DELTA;

        let curve = TransferCurve::piecewise([
            CurveSegment::new(
                DELTA,
                CurveSegmentKind::Linear {
                    scale: LINEAR_SCALE,
                    offset: -LINEAR_SCALE * 4.0 / 29.0,
                },
            ),
            CurveSegment::new(
                f64::INFINITY,
                CurveSegmentKind::PowerAffine {
                    scale: 1.0,
                    offset: 0.0,
                    exponent: 3.0,
                    output_scale: 1.0,
                    output_offset: 0.0,
                },
            ),
        ])?;
        let y_offset = 16.0 / 116.0;
        Transfer::new([
            Transfer::matrix(Self::LAB_TO_F_XYZ, [y_offset; 3])?,
            Transfer::curves(vec![curve; 3])?,
            Transfer::matrix(Self::NORMALIZED_XYZ_TO_XYZ_D50, [0.0; 3])?,
        ])
    }
}

impl ColorProfile for LabD50Profile {
    fn channels(&self) -> usize {
        3
    }

    fn connection_space(&self) -> ConnectionSpace {
        ConnectionSpace::LabD50
    }

    fn transfer_to_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(Transfer::IDENTITY)
    }

    fn transfer_from_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(Transfer::IDENTITY)
    }

    fn direct_transfer_to(
        &self,
        destination: &ColorSpace,
    ) -> Result<Option<Transfer>, SpaceConnectionError> {
        if *destination == ColorSpace::from(LchD50) {
            Ok(Some(LchD50::transfer_from_lab_d50()?))
        } else {
            Ok(None)
        }
    }
}
