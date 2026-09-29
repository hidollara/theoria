use super::{ColorProfile, ConnectionSpace, SpaceConnectionError, TypedColorSpace, XyzD65};
use crate::model::Rgb;
use crate::transfer::{Transfer, TransferCurve, TransferError, TransferMatrix, TransferStage};

named_space!(Rec2020, Rgb);

impl Rec2020 {
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
        Ok(XyzD65::transfer_to_rec2020()?)
    }
}
