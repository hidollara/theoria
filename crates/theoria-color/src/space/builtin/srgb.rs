use super::{
    ColorProfile, ColorSpace, Pcs, SpaceConnectionError, Transfer, TransferCurve, TypedColorSpace,
    pcs_to_rgb_transfer, rgb_direct_to, rgb_to_pcs_transfer, rgb_to_xyz_d65_transfer,
    xyz_d65_to_rgb_transfer,
};
use crate::model::Rgb;

named_space!(Srgb, Rgb);

impl Srgb {
    // Exact rational matrices from CSS Color 4's color-conversion sample code.
    // The linear and encoded sRGB spaces share these primaries.
    pub(crate) const TO_XYZ_D65: [[f64; 3]; 3] = [
        [506752.0 / 1228815.0, 87881.0 / 245763.0, 12673.0 / 70218.0],
        [87098.0 / 409605.0, 175762.0 / 245763.0, 12673.0 / 175545.0],
        [7918.0 / 409605.0, 87881.0 / 737289.0, 1001167.0 / 1053270.0],
    ];
    pub(crate) const FROM_XYZ_D65: [[f64; 3]; 3] = [
        [12831.0 / 3959.0, -329.0 / 214.0, -1974.0 / 3959.0],
        [
            -851781.0 / 878810.0,
            1648619.0 / 878810.0,
            36519.0 / 878810.0,
        ],
        [705.0 / 12673.0, -2585.0 / 12673.0, 705.0 / 667.0],
    ];
}

builtin_space!(Srgb, SrgbProfile);

impl ColorProfile for SrgbProfile {
    fn model(&self) -> crate::model::ColorModel {
        crate::model::ColorModel::Rgb
    }

    fn pcs(&self) -> Pcs {
        Pcs::XyzD50
    }

    fn space_to_pcs_transfer(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(rgb_to_pcs_transfer(rgb_to_xyz_d65_transfer(
            Srgb::TO_XYZ_D65,
            TransferCurve::SRGB_DECODE,
        )?)?)
    }

    fn pcs_to_space_transfer(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(pcs_to_rgb_transfer(xyz_d65_to_rgb_transfer(
            Srgb::FROM_XYZ_D65,
            TransferCurve::SRGB_ENCODE,
        )?)?)
    }

    fn direct_transfer_to(
        &self,
        destination: &ColorSpace,
    ) -> Result<Option<Transfer>, SpaceConnectionError> {
        rgb_direct_to(
            super::LinearSrgb.into(),
            TransferCurve::SRGB_DECODE,
            destination,
        )
    }
}
