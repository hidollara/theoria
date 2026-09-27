use super::{
    ColorSpace, IccRenderingIntent, Pcs, ProfileConnectionError, ProfileDefinition, Transfer,
    TransferCurve, TypedColorSpace, pcs_to_rgb_transfer, require_relative, rgb_direct_to,
    rgb_to_pcs_transfer, rgb_to_xyz_d65_transfer, xyz_d65_to_rgb_transfer,
};
use crate::model::Rgb;

named_space!(DisplayP3, Rgb);

impl DisplayP3 {
    // Exact rational matrices from CSS Color 4's color-conversion sample code.
    // The linear and encoded Display P3 spaces share these primaries.
    pub(crate) const TO_XYZ_D65: [[f64; 3]; 3] = [
        [
            608311.0 / 1250200.0,
            189793.0 / 714400.0,
            198249.0 / 1000160.0,
        ],
        [
            35783.0 / 156275.0,
            247089.0 / 357200.0,
            198249.0 / 2500400.0,
        ],
        [0.0, 32229.0 / 714400.0, 5220557.0 / 5000800.0],
    ];
    pub(crate) const FROM_XYZ_D65: [[f64; 3]; 3] = [
        [
            446124.0 / 178915.0,
            -333277.0 / 357830.0,
            -72051.0 / 178915.0,
        ],
        [-14852.0 / 17905.0, 63121.0 / 35810.0, 423.0 / 17905.0],
        [11844.0 / 330415.0, -50337.0 / 660830.0, 316169.0 / 330415.0],
    ];
}

predefined_space!(DisplayP3, DisplayP3Profile);

impl ProfileDefinition for DisplayP3Profile {
    fn model(&self) -> crate::model::ColorModel {
        crate::model::ColorModel::Rgb
    }

    fn pcs(&self) -> Pcs {
        Pcs::XyzD50
    }

    fn to_pcs_transfer(
        &self,
        intent: IccRenderingIntent,
    ) -> Result<Transfer, ProfileConnectionError> {
        require_relative(intent)?;
        Ok(rgb_to_pcs_transfer(
            rgb_to_xyz_d65_transfer(DisplayP3::TO_XYZ_D65, TransferCurve::SRGB_DECODE)?,
            self.pcs(),
        )?)
    }

    fn from_pcs_transfer(
        &self,
        intent: IccRenderingIntent,
    ) -> Result<Transfer, ProfileConnectionError> {
        require_relative(intent)?;
        Ok(pcs_to_rgb_transfer(
            self.pcs(),
            xyz_d65_to_rgb_transfer(DisplayP3::FROM_XYZ_D65, TransferCurve::SRGB_ENCODE)?,
        )?)
    }

    fn direct_transfer_to(
        &self,
        destination: &ColorSpace,
        intent: IccRenderingIntent,
    ) -> Result<Option<Transfer>, ProfileConnectionError> {
        rgb_direct_to(
            DisplayP3.into(),
            super::LinearDisplayP3.into(),
            TransferCurve::SRGB_DECODE,
            destination,
            intent,
        )
    }
}
