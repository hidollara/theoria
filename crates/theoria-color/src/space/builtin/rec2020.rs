use super::{ColorProfile, ConnectionSpace, SpaceConnectionError, TypedColorSpace};
use crate::model::Rgb;
use crate::transfer::{Transfer, TransferCurve, TransferError, TransferMatrix, TransferStage};

named_space!(Rec2020, Rgb);

impl Rec2020 {
    const FROM_XYZ_D65: [[f64; 3]; 3] = [
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

    pub(crate) fn transfer_to_xyz_d65() -> Result<Transfer, TransferError> {
        let decode = TransferStage::curves(vec![TransferCurve::signed_power(2.4)?; 3])?;
        let matrix = TransferStage::Matrix(TransferMatrix::new(
            3,
            3,
            [
                63426534.0 / 99577255.0,
                20160776.0 / 139408157.0,
                47086771.0 / 278816314.0,
                26158966.0 / 99577255.0,
                472592308.0 / 697040785.0,
                8267143.0 / 139408157.0,
                0.0,
                19567812.0 / 697040785.0,
                295819943.0 / 278816314.0,
            ],
            [0.0; 3],
        )?);
        Transfer::new([decode, matrix])
    }

    pub(crate) fn transfer_from_xyz_d65() -> Result<Transfer, TransferError> {
        super::rgb_from_xyz(
            Self::FROM_XYZ_D65,
            Some(TransferCurve::signed_power(1.0 / 2.4)?),
        )
    }
}

builtin_space!(Rec2020, Rec2020Profile);

impl ColorProfile for Rec2020Profile {
    fn channels(&self) -> usize {
        3
    }

    fn connection_space(&self) -> ConnectionSpace {
        ConnectionSpace::XyzD65
    }

    fn transfer_to_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(Rec2020::transfer_to_xyz_d65()?)
    }

    fn transfer_from_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(Rec2020::transfer_from_xyz_d65()?)
    }
}
