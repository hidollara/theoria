use super::{ColorProfile, ConnectionSpace, SpaceConnectionError, Srgb, TypedColorSpace, XyzD65};
use crate::model::Hsl;
use crate::transfer::{ColorFunction, Transfer, TransferError, TransferStage};

named_space!(HslSrgb, Hsl);

impl HslSrgb {
    pub(crate) fn transfer_to_srgb() -> Result<Transfer, TransferError> {
        Transfer::new([TransferStage::ColorFunction(ColorFunction::HslToRgb)])
    }
}

builtin_space!(HslSrgb, HslSrgbProfile);

impl ColorProfile for HslSrgbProfile {
    fn model(&self) -> crate::model::ColorModel {
        crate::model::ColorModel::Hsl
    }

    fn connection_space(&self) -> ConnectionSpace {
        ConnectionSpace::XyzD65
    }

    fn transfer_to_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(HslSrgb::transfer_to_srgb()?.then(&Srgb::transfer_to_xyz_d65()?)?)
    }

    fn transfer_from_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(
            XyzD65::transfer_to_srgb()?.then(&Transfer::new([TransferStage::ColorFunction(
                ColorFunction::RgbToHsl,
            )])?)?,
        )
    }
}
