use std::sync::LazyLock;

use super::{ColorSpace, ConnectionSpace, LchD50, SpaceConnectionError, TypedColorSpace};
use crate::model::Lab;
use crate::transfer::{
    CurveSegment, CurveSegmentKind, ICC_PCS_D50_XYZ, Transfer, TransferCurve, TransferError,
};

builtin_space!(LabD50);

impl LabD50 {
    const XYZ_D50_TO_NORMALIZED_XYZ: [[f64; 3]; 3] = [
        [1.0 / ICC_PCS_D50_XYZ[0], 0.0, 0.0],
        [0.0, 1.0 / ICC_PCS_D50_XYZ[1], 0.0],
        [0.0, 0.0, 1.0 / ICC_PCS_D50_XYZ[2]],
    ];

    // Maps f(X/Xn), f(Y/Yn), f(Z/Zn) to Lab coordinates.
    const F_XYZ_TO_LAB: [[f64; 3]; 3] = [
        [0.0, 116.0, 0.0],
        [500.0, -500.0, 0.0],
        [0.0, 200.0, -200.0],
    ];

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
    pub(crate) fn transfer_from_xyz_d50() -> Transfer {
        static TRANSFER: LazyLock<Transfer> = LazyLock::new(|| {
            LabD50::build_transfer_from_xyz_d50()
                .expect("built-in XYZ D50 to Lab D50 transfer is valid")
        });
        TRANSFER.clone()
    }

    fn build_transfer_from_xyz_d50() -> Result<Transfer, TransferError> {
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
            Transfer::matrix(Self::XYZ_D50_TO_NORMALIZED_XYZ, [0.0; 3])?,
            Transfer::curves(vec![curve; 3])?,
            Transfer::matrix(Self::F_XYZ_TO_LAB, [-16.0, 0.0, 0.0])?,
        ])
    }
}

impl TypedColorSpace for LabD50 {
    type Model = Lab;

    fn connection_space() -> ConnectionSpace {
        ConnectionSpace::LabD50
    }

    fn transfer_to_connection_space() -> Result<Transfer, SpaceConnectionError> {
        Ok(Transfer::IDENTITY)
    }

    fn transfer_from_connection_space() -> Result<Transfer, SpaceConnectionError> {
        Ok(Transfer::IDENTITY)
    }

    fn direct_transfer_to(
        destination: &ColorSpace,
    ) -> Result<Option<Transfer>, SpaceConnectionError> {
        if *destination == ColorSpace::from(LchD50) {
            Ok(Some(LchD50::transfer_from_lab_d50()?))
        } else {
            Ok(None)
        }
    }
}
