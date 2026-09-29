use super::{
    ColorProfile, ConnectionSpace, OklabD65, SpaceConnectionError, TypedColorSpace, XyzD65,
};
use crate::model::Oklch;
use crate::transfer::{ColorFunction, Transfer, TransferError, TransferStage};

named_space!(OklchD65, Oklch);

impl OklchD65 {
    pub(crate) fn transfer_to_oklab_d65() -> Result<Transfer, TransferError> {
        Transfer::new([TransferStage::ColorFunction(
            ColorFunction::PolarToCartesian,
        )])
    }
}

builtin_space!(OklchD65, OklchD65Profile);

impl ColorProfile for OklchD65Profile {
    fn model(&self) -> crate::model::ColorModel {
        crate::model::ColorModel::Oklch
    }

    fn connection_space(&self) -> ConnectionSpace {
        ConnectionSpace::XyzD65
    }

    fn transfer_to_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(OklchD65::transfer_to_oklab_d65()?.then(&OklabD65::transfer_to_xyz_d65()?)?)
    }

    fn transfer_from_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(
            XyzD65::transfer_to_oklab()?.then(&Transfer::new([TransferStage::ColorFunction(
                ColorFunction::CartesianToPolar,
            )])?)?,
        )
    }
}
