use super::{ColorProfile, ConnectionSpace, SpaceConnectionError, TypedColorSpace, XyzD65};
use crate::model::Oklab as OklabModel;
use crate::transfer::{Transfer, TransferCurve, TransferError, TransferMatrix, TransferStage};

named_space!(OklabD65, OklabModel);

impl OklabD65 {
    pub(crate) fn transfer_to_xyz_d65() -> Result<Transfer, TransferError> {
        Transfer::new([
            TransferStage::Matrix(TransferMatrix::new(
                3,
                3,
                [
                    1.0,
                    0.3963377773761749,
                    0.2158037573099136,
                    1.0,
                    -0.1055613458156586,
                    -0.0638541728258133,
                    1.0,
                    -0.0894841775298119,
                    -1.2914855480194092,
                ],
                [0.0; 3],
            )?),
            TransferStage::curves(vec![TransferCurve::signed_power(3.0)?; 3])?,
            TransferStage::Matrix(TransferMatrix::new(
                3,
                3,
                [
                    1.2268798758459243,
                    -0.5578149944602171,
                    0.2813910456659647,
                    -0.0405757452148008,
                    1.112286803280317,
                    -0.0717110580655164,
                    -0.0763729366746601,
                    -0.4214933324022432,
                    1.5869240198367816,
                ],
                [0.0; 3],
            )?),
        ])
    }
}

builtin_space!(OklabD65, OklabD65Profile);

impl ColorProfile for OklabD65Profile {
    fn model(&self) -> crate::model::ColorModel {
        crate::model::ColorModel::Oklab
    }

    fn connection_space(&self) -> ConnectionSpace {
        ConnectionSpace::XyzD65
    }

    fn transfer_to_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(OklabD65::transfer_to_xyz_d65()?)
    }

    fn transfer_from_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(XyzD65::transfer_to_oklab()?)
    }
}
