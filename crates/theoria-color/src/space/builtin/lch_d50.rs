use super::{ColorProfile, ConnectionSpace, LabD50, SpaceConnectionError, TypedColorSpace, XyzD50};
use crate::model::Lch;
use crate::transfer::{ColorFunction, Transfer, TransferError, TransferStage};

named_space!(LchD50, Lch);

impl LchD50 {
    pub(crate) fn transfer_to_lab_d50() -> Result<Transfer, TransferError> {
        Transfer::new([TransferStage::ColorFunction(
            ColorFunction::PolarToCartesian,
        )])
    }
}

builtin_space!(LchD50, LchD50Profile);

impl ColorProfile for LchD50Profile {
    fn model(&self) -> crate::model::ColorModel {
        crate::model::ColorModel::Lch
    }

    fn connection_space(&self) -> ConnectionSpace {
        ConnectionSpace::XyzD50
    }

    fn transfer_to_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(LchD50::transfer_to_lab_d50()?.then(&LabD50::transfer_to_xyz_d50())?)
    }

    fn transfer_from_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(
            XyzD50::transfer_to_lab_d50().then(&Transfer::new([TransferStage::ColorFunction(
                ColorFunction::CartesianToPolar,
            )])?)?,
        )
    }
}
