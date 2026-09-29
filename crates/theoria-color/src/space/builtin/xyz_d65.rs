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
    const TO_A98_RGB: [[f64; 3]; 3] = [
        [
            1829569.0 / 896150.0,
            -506331.0 / 896150.0,
            -308931.0 / 896150.0,
        ],
        [
            -851781.0 / 878810.0,
            1648619.0 / 878810.0,
            36519.0 / 878810.0,
        ],
        [
            16779.0 / 1248040.0,
            -147721.0 / 1248040.0,
            1266979.0 / 1248040.0,
        ],
    ];
    const TO_REC2020: [[f64; 3]; 3] = [
        [
            30757411.0 / 17917100.0,
            -6372589.0 / 17917100.0,
            -4539589.0 / 17917100.0,
        ],
        [
            -19765991.0 / 29648200.0,
            47925759.0 / 29648200.0,
            467509.0 / 29648200.0,
        ],
        [
            792561.0 / 44930125.0,
            -1921689.0 / 44930125.0,
            42328811.0 / 44930125.0,
        ],
    ];

    pub(crate) fn transfer_to_srgb() -> Result<Transfer, TransferError> {
        Self::transfer_to_rgb(Self::TO_SRGB, Some(TransferCurve::SRGB_ENCODE))
    }

    pub(crate) fn transfer_to_linear_srgb() -> Result<Transfer, TransferError> {
        Self::transfer_to_rgb(Self::TO_SRGB, None)
    }

    pub(crate) fn transfer_to_display_p3() -> Result<Transfer, TransferError> {
        Self::transfer_to_rgb(Self::TO_DISPLAY_P3, Some(TransferCurve::SRGB_ENCODE))
    }

    pub(crate) fn transfer_to_linear_display_p3() -> Result<Transfer, TransferError> {
        Self::transfer_to_rgb(Self::TO_DISPLAY_P3, None)
    }

    pub(crate) fn transfer_to_a98_rgb() -> Result<Transfer, TransferError> {
        Self::transfer_to_rgb(
            Self::TO_A98_RGB,
            Some(TransferCurve::signed_power(256.0 / 563.0)?),
        )
    }

    pub(crate) fn transfer_to_rec2020() -> Result<Transfer, TransferError> {
        Self::transfer_to_rgb(
            Self::TO_REC2020,
            Some(TransferCurve::signed_power(1.0 / 2.4)?),
        )
    }

    pub(crate) fn transfer_to_oklab() -> Result<Transfer, TransferError> {
        Transfer::new([
            TransferStage::Matrix(TransferMatrix::new(
                3,
                3,
                [
                    0.819022437996703,
                    0.3619062600528904,
                    -0.1288737815209879,
                    0.0329836539323885,
                    0.9292868615863434,
                    0.0361446663506424,
                    0.0481771893596242,
                    0.2642395317527308,
                    0.6335478284694309,
                ],
                [0.0; 3],
            )?),
            TransferStage::curves(vec![TransferCurve::signed_power(1.0 / 3.0)?; 3])?,
            TransferStage::Matrix(TransferMatrix::new(
                3,
                3,
                [
                    0.210454268309314,
                    0.7936177747023054,
                    -0.0040720430116193,
                    1.9779985324311684,
                    -2.42859224204858,
                    0.450593709617411,
                    0.0259040424655478,
                    0.7827717124575296,
                    -0.8086757549230774,
                ],
                [0.0; 3],
            )?),
        ])
    }

    fn transfer_to_rgb(
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

    pub(crate) fn transfer_to_xyz_d50() -> Transfer {
        static TRANSFER: LazyLock<Transfer> = LazyLock::new(|| {
            XyzD65::build_transfer_to_xyz_d50().expect("built-in D65 to D50 adaptation is valid")
        });
        TRANSFER.clone()
    }

    fn build_transfer_to_xyz_d50() -> Result<Transfer, TransferError> {
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
