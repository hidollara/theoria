use super::{
    ColorProfile, ColorSpace, ConnectionSpace, HwbSrgb, LinearSrgb, SpaceConnectionError, Srgb,
    TypedColorSpace,
};
use crate::model::Hls;
use crate::transfer::{ColorFunction, Transfer, TransferError};

// The named HSL space exposes canonical HLS-ordered coordinates.
named_space!(HslSrgb, Hls);

impl HslSrgb {
    pub(crate) fn transfer_to_srgb() -> Result<Transfer, TransferError> {
        Ok(Transfer::color_function(ColorFunction::HlsToRgb))
    }

    pub(crate) fn transfer_from_srgb() -> Result<Transfer, TransferError> {
        Ok(Transfer::color_function(ColorFunction::RgbToHls))
    }

    pub(crate) fn transfer_to_xyz_d65() -> Result<Transfer, TransferError> {
        Self::transfer_to_srgb()?.then(&Srgb::transfer_to_xyz_d65()?)
    }

    pub(crate) fn transfer_from_xyz_d65() -> Result<Transfer, TransferError> {
        Srgb::transfer_from_xyz_d65()?.then(&Self::transfer_from_srgb()?)
    }
}

builtin_space!(HslSrgb, HslSrgbProfile);

impl ColorProfile for HslSrgbProfile {
    fn model(&self) -> crate::model::ColorModel {
        <Hls as crate::model::TypedColorModel>::MODEL
    }

    fn polar_hue_index(&self) -> Option<usize> {
        Some(0)
    }

    fn hue_is_powerless(&self, coordinates: &[f64]) -> bool {
        coordinates[2] <= 0.0
    }

    fn connection_space(&self) -> ConnectionSpace {
        ConnectionSpace::XyzD65
    }

    fn transfer_to_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(HslSrgb::transfer_to_xyz_d65()?)
    }

    fn transfer_from_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(HslSrgb::transfer_from_xyz_d65()?)
    }

    fn direct_transfer_to(
        &self,
        destination: &ColorSpace,
    ) -> Result<Option<Transfer>, SpaceConnectionError> {
        if *destination == ColorSpace::from(Srgb) {
            Ok(Some(HslSrgb::transfer_to_srgb()?))
        } else if *destination == ColorSpace::from(LinearSrgb) {
            Ok(Some(
                HslSrgb::transfer_to_srgb()?.then(&Srgb::transfer_to_linear_srgb()?)?,
            ))
        } else if *destination == ColorSpace::from(HwbSrgb) {
            Ok(Some(
                HslSrgb::transfer_to_srgb()?.then(&HwbSrgb::transfer_from_srgb()?)?,
            ))
        } else {
            Ok(None)
        }
    }
}
