use super::{ColorProfile, ConnectionSpace, SpaceConnectionError, Srgb, TypedColorSpace};
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
    fn channels(&self) -> usize {
        3
    }

    fn polar_hue_index(&self) -> Option<usize> {
        Some(0)
    }

    fn hue_is_powerless(&self, coordinates: &[f64]) -> bool {
        coordinates[1] <= 0.0
    }

    fn connection_space(&self) -> ConnectionSpace {
        ConnectionSpace::XyzD65
    }

    fn transfer_to_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(HslSrgb::transfer_to_srgb()?.then(&Srgb::transfer_to_xyz_d65()?)?)
    }

    fn transfer_from_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(
            Srgb::transfer_from_xyz_d65()?.then(&Transfer::new([
                TransferStage::ColorFunction(ColorFunction::RgbToHsl),
            ])?)?,
        )
    }
}
