use super::{ColorProfile, ConnectionSpace, SpaceConnectionError, Srgb, TypedColorSpace, XyzD65};
use crate::model::Hwb;
use crate::transfer::{ColorFunction, Transfer, TransferError, TransferStage};

named_space!(HwbSrgb, Hwb);

impl HwbSrgb {
    pub(crate) fn transfer_to_srgb() -> Result<Transfer, TransferError> {
        Transfer::new([TransferStage::ColorFunction(ColorFunction::HwbToRgb)])
    }
}

builtin_space!(HwbSrgb, HwbSrgbProfile);

impl ColorProfile for HwbSrgbProfile {
    fn model(&self) -> crate::model::ColorModel {
        crate::model::ColorModel::Hwb
    }

    fn connection_space(&self) -> ConnectionSpace {
        ConnectionSpace::XyzD65
    }

    fn transfer_to_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(HwbSrgb::transfer_to_srgb()?.then(&Srgb::transfer_to_xyz_d65()?)?)
    }

    fn transfer_from_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(
            XyzD65::transfer_to_srgb()?.then(&Transfer::new([TransferStage::ColorFunction(
                ColorFunction::RgbToHwb,
            )])?)?,
        )
    }
}
