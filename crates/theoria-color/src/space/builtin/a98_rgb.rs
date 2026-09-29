use super::{ColorProfile, ConnectionSpace, SpaceConnectionError, TypedColorSpace, XyzD65};
use crate::model::Rgb;
use crate::transfer::{Transfer, TransferCurve, TransferError, TransferMatrix, TransferStage};

named_space!(A98Rgb, Rgb);

impl A98Rgb {
    pub(crate) fn transfer_to_xyz_d65() -> Result<Transfer, TransferError> {
        let decode = TransferStage::curves(vec![TransferCurve::signed_power(563.0 / 256.0)?; 3])?;
        let matrix = TransferStage::Matrix(TransferMatrix::new(
            3,
            3,
            [
                573536.0 / 994567.0,
                263643.0 / 1420810.0,
                187206.0 / 994567.0,
                591459.0 / 1989134.0,
                6239551.0 / 9945670.0,
                374412.0 / 4972835.0,
                53769.0 / 1989134.0,
                351524.0 / 4972835.0,
                4929758.0 / 4972835.0,
            ],
            [0.0; 3],
        )?);
        Transfer::new([decode, matrix])
    }
}

builtin_space!(A98Rgb, A98RgbProfile);

impl ColorProfile for A98RgbProfile {
    fn model(&self) -> crate::model::ColorModel {
        crate::model::ColorModel::Rgb
    }

    fn connection_space(&self) -> ConnectionSpace {
        ConnectionSpace::XyzD65
    }

    fn transfer_to_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(A98Rgb::transfer_to_xyz_d65()?)
    }

    fn transfer_from_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(XyzD65::transfer_to_a98_rgb()?)
    }
}
