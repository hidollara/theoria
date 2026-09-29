use std::sync::LazyLock;

use super::{ColorProfile, ConnectionSpace, SpaceConnectionError, TypedColorSpace};
use crate::model::Xyz;
use crate::transfer::{Transfer, TransferCurve, TransferError, TransferMatrix, TransferStage};

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
    // Exact rational matrices from CSS Color 4's color-conversion sample code.
    const TO_SRGB: [[f64; 3]; 3] = [
        [12831.0 / 3959.0, -329.0 / 214.0, -1974.0 / 3959.0],
        [
            -851781.0 / 878810.0,
            1648619.0 / 878810.0,
            36519.0 / 878810.0,
        ],
        [705.0 / 12673.0, -2585.0 / 12673.0, 705.0 / 667.0],
    ];
    const TO_DISPLAY_P3: [[f64; 3]; 3] = [
        [
            446124.0 / 178915.0,
            -333277.0 / 357830.0,
            -72051.0 / 178915.0,
        ],
        [-14852.0 / 17905.0, 63121.0 / 35810.0, 423.0 / 17905.0],
        [11844.0 / 330415.0, -50337.0 / 660830.0, 316169.0 / 330415.0],
    ];

    pub(crate) fn to_srgb_transfer() -> Result<Transfer, TransferError> {
        Self::to_rgb_transfer(Self::TO_SRGB, Some(TransferCurve::SRGB_ENCODE))
    }

    pub(crate) fn to_linear_srgb_transfer() -> Result<Transfer, TransferError> {
        Self::to_rgb_transfer(Self::TO_SRGB, None)
    }

    pub(crate) fn to_display_p3_transfer() -> Result<Transfer, TransferError> {
        Self::to_rgb_transfer(Self::TO_DISPLAY_P3, Some(TransferCurve::SRGB_ENCODE))
    }

    pub(crate) fn to_linear_display_p3_transfer() -> Result<Transfer, TransferError> {
        Self::to_rgb_transfer(Self::TO_DISPLAY_P3, None)
    }

    fn to_rgb_transfer(
        matrix: [[f64; 3]; 3],
        curve: Option<TransferCurve>,
    ) -> Result<Transfer, TransferError> {
        let matrix = TransferStage::Matrix(TransferMatrix::new(
            3,
            3,
            matrix.into_iter().flatten().collect::<Vec<_>>(),
            [0.0; 3],
        )?);
        match curve {
            Some(curve) => Transfer::new([matrix, TransferStage::curves(vec![curve; 3])?]),
            None => Transfer::new([matrix]),
        }
    }
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
