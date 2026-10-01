use super::{
    ColorProfile, ColorSpace, ConnectionSpace, HslSrgb, LinearSrgb, SpaceConnectionError, Srgb,
    TypedColorSpace,
};
use crate::model::Hwb;
use crate::transfer::{ColorFunction, Transfer, TransferError, TransferStage};

named_space!(HwbSrgb, Hwb);

impl HwbSrgb {
    pub(crate) fn transfer_to_srgb() -> Result<Transfer, TransferError> {
        Transfer::new([TransferStage::ColorFunction(ColorFunction::HwbToRgb)])
    }

    pub(crate) fn transfer_from_srgb() -> Result<Transfer, TransferError> {
        Transfer::new([TransferStage::ColorFunction(ColorFunction::RgbToHwb)])
    }

    pub(crate) fn transfer_to_xyz_d65() -> Result<Transfer, TransferError> {
        Self::transfer_to_srgb()?.then(&Srgb::transfer_to_xyz_d65()?)
    }

    pub(crate) fn transfer_from_xyz_d65() -> Result<Transfer, TransferError> {
        Srgb::transfer_from_xyz_d65()?.then(&Self::transfer_from_srgb()?)
    }
}

builtin_space!(HwbSrgb, HwbSrgbProfile);

impl ColorProfile for HwbSrgbProfile {
    fn channels(&self) -> usize {
        3
    }

    fn polar_hue_index(&self) -> Option<usize> {
        Some(0)
    }

    fn hue_is_powerless(&self, coordinates: &[f64]) -> bool {
        coordinates[1] + coordinates[2] >= 100.0
    }

    fn connection_space(&self) -> ConnectionSpace {
        ConnectionSpace::XyzD65
    }

    fn transfer_to_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(HwbSrgb::transfer_to_xyz_d65()?)
    }

    fn transfer_from_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(HwbSrgb::transfer_from_xyz_d65()?)
    }

    fn direct_transfer_to(
        &self,
        destination: &ColorSpace,
    ) -> Result<Option<Transfer>, SpaceConnectionError> {
        if *destination == ColorSpace::from(Srgb) {
            Ok(Some(HwbSrgb::transfer_to_srgb()?))
        } else if *destination == ColorSpace::from(LinearSrgb) {
            Ok(Some(
                HwbSrgb::transfer_to_srgb()?.then(&Srgb::transfer_to_linear_srgb()?)?,
            ))
        } else if *destination == ColorSpace::from(HslSrgb) {
            Ok(Some(
                HwbSrgb::transfer_to_srgb()?.then(&HslSrgb::transfer_from_srgb()?)?,
            ))
        } else {
            Ok(None)
        }
    }
}
