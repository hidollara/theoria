use std::sync::LazyLock;

use super::{ColorProfile, ConnectionSpace, SpaceConnectionError, TypedColorSpace};
use crate::model::Xyz;
use crate::transfer::{Transfer, TransferError};

named_space!(XyzD65, Xyz);
builtin_space!(XyzD65, XyzD65Profile);

impl XyzD65 {
    // Linear Bradford adaptation from the CSS Color 4 conversion sample.
    // https://www.w3.org/TR/css-color-4/#color-conversion-code
    const TO_XYZ_D50: [[f64; 3]; 3] = [
        [
            1.0479297925449969,
            0.022946870601609652,
            -0.05019226628920524,
        ],
        [
            0.02962780877005599,
            0.9904344267538799,
            -0.017073799063418826,
        ],
        [
            -0.009243040646204504,
            0.015055191490298152,
            0.7518742814281371,
        ],
    ];
    pub(crate) fn transfer_to_xyz_d50() -> Transfer {
        static TRANSFER: LazyLock<Transfer> = LazyLock::new(|| {
            XyzD65::build_transfer_to_xyz_d50().expect("built-in D65 to D50 adaptation is valid")
        });
        TRANSFER.clone()
    }

    fn build_transfer_to_xyz_d50() -> Result<Transfer, TransferError> {
        Transfer::matrix(Self::TO_XYZ_D50, [0.0; 3])
    }
}

impl ColorProfile for XyzD65Profile {
    fn model(&self) -> crate::model::ColorModel {
        <Xyz as crate::model::TypedColorModel>::MODEL
    }

    fn connection_space(&self) -> ConnectionSpace {
        ConnectionSpace::XyzD65
    }

    fn transfer_to_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(Transfer::IDENTITY)
    }

    fn transfer_from_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(Transfer::IDENTITY)
    }
}
