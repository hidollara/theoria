use std::sync::LazyLock;

use super::{
    ColorProfile, ColorSpace, Pcs, SpaceConnectionError, Transfer, TypedColorSpace, XyzD50,
};
use crate::model::Xyz;
use crate::transfer::{TransferError, TransferMatrix, TransferStage};

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
    pub(crate) fn to_xyz_d50_transfer() -> Transfer {
        static TRANSFER: LazyLock<Transfer> = LazyLock::new(|| {
            XyzD65::build_to_xyz_d50_transfer().expect("built-in D65 to D50 adaptation is valid")
        });
        TRANSFER.clone()
    }

    fn build_to_xyz_d50_transfer() -> Result<Transfer, TransferError> {
        Transfer::new([TransferStage::Matrix(TransferMatrix::new(
            3,
            3,
            Self::TO_XYZ_D50.into_iter().flatten().collect::<Vec<_>>(),
            [0.0; 3],
        )?)])
    }
}

impl ColorProfile for XyzD65Profile {
    fn model(&self) -> crate::model::ColorModel {
        crate::model::ColorModel::Xyz
    }

    fn pcs(&self) -> Pcs {
        Pcs::XyzD50
    }

    fn space_to_pcs_transfer(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(XyzD65::to_xyz_d50_transfer())
    }

    fn pcs_to_space_transfer(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(XyzD50::to_xyz_d65_transfer())
    }

    fn direct_transfer_to(
        &self,
        destination: &ColorSpace,
    ) -> Result<Option<Transfer>, SpaceConnectionError> {
        if *destination == ColorSpace::from(XyzD50) {
            Ok(Some(XyzD65::to_xyz_d50_transfer()))
        } else {
            Ok(None)
        }
    }
}
