use std::sync::LazyLock;

use super::{ColorProfile, ConnectionSpace, SpaceConnectionError, TypedColorSpace};
use crate::model::Xyz;
use crate::transfer::{Transfer, TransferError};

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

    pub(crate) fn transfer_to_xyz_d65() -> Transfer {
        static TRANSFER: LazyLock<Transfer> = LazyLock::new(|| {
            XyzD50::build_transfer_to_xyz_d65().expect("built-in D50 to D65 adaptation is valid")
        });
        TRANSFER.clone()
    }

    fn build_transfer_to_xyz_d65() -> Result<Transfer, TransferError> {
        Transfer::matrix(Self::TO_XYZ_D65, [0.0; 3])
    }
}

impl ColorProfile for XyzD50Profile {
    fn model(&self) -> crate::model::ColorModel {
        <Xyz as crate::model::TypedColorModel>::MODEL
    }

    fn connection_space(&self) -> ConnectionSpace {
        ConnectionSpace::XyzD50
    }

    fn transfer_to_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(Transfer::IDENTITY)
    }

    fn transfer_from_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(Transfer::IDENTITY)
    }
}
