use super::{ColorSpace, ConnectionSpace, DisplayP3, SpaceConnectionError, TypedColorSpace};
use crate::model::Rgb;
use crate::transfer::{Transfer, TransferError};

builtin_space!(LinearDisplayP3);

impl LinearDisplayP3 {
    // Exact rational matrix from CSS Color 4's color-conversion sample code.
    const TO_XYZ_D65: [[f64; 3]; 3] = [
        [
            608311.0 / 1250200.0,
            189793.0 / 714400.0,
            198249.0 / 1000160.0,
        ],
        [
            35783.0 / 156275.0,
            247089.0 / 357200.0,
            198249.0 / 2500400.0,
        ],
        [0.0, 32229.0 / 714400.0, 5220557.0 / 5000800.0],
    ];

    const FROM_XYZ_D65: [[f64; 3]; 3] = [
        [
            446124.0 / 178915.0,
            -333277.0 / 357830.0,
            -72051.0 / 178915.0,
        ],
        [-14852.0 / 17905.0, 63121.0 / 35810.0, 423.0 / 17905.0],
        [11844.0 / 330415.0, -50337.0 / 660830.0, 316169.0 / 330415.0],
    ];

    pub(crate) fn transfer_to_xyz_d65() -> Result<Transfer, TransferError> {
        Transfer::matrix(Self::TO_XYZ_D65, [0.0; 3])
    }

    pub(crate) fn transfer_from_xyz_d65() -> Result<Transfer, TransferError> {
        Transfer::matrix(Self::FROM_XYZ_D65, [0.0; 3])
    }
}

impl TypedColorSpace for LinearDisplayP3 {
    type Model = Rgb;

    fn connection_space() -> ConnectionSpace {
        ConnectionSpace::XyzD65
    }

    fn transfer_to_connection_space() -> Result<Transfer, SpaceConnectionError> {
        Ok(LinearDisplayP3::transfer_to_xyz_d65()?)
    }

    fn transfer_from_connection_space() -> Result<Transfer, SpaceConnectionError> {
        Ok(LinearDisplayP3::transfer_from_xyz_d65()?)
    }

    fn direct_transfer_to(
        destination: &ColorSpace,
    ) -> Result<Option<Transfer>, SpaceConnectionError> {
        if *destination == ColorSpace::from(DisplayP3) {
            Ok(Some(DisplayP3::transfer_from_linear_display_p3()?))
        } else {
            Ok(None)
        }
    }
}
