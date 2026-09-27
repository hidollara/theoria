use super::{
    ColorProfile, ColorSpace, ColorTransform, DisplayP3, IccRenderingIntent, Pcs,
    ProfileConnectionError, ProfileDefinition, TransferCurve, TypedColorSpace, require_relative,
    rgb_direct_to, rgb_to_xyz_d65_transfer, xyz_d65_to_rgb_transfer,
};
use crate::model::Rgb;

named_space!(LinearDisplayP3, Rgb);

impl LinearDisplayP3 {
    pub(crate) const TO_XYZ_D65: [[f64; 3]; 3] = DisplayP3::TO_XYZ_D65;
    pub(crate) const FROM_XYZ_D65: [[f64; 3]; 3] = DisplayP3::FROM_XYZ_D65;
}

predefined_space!(
    LinearDisplayP3,
    LinearDisplayP3Profile,
    LINEAR_DISPLAY_P3_KEY
);

impl ProfileDefinition for LinearDisplayP3Profile {
    fn space(&self) -> ColorSpace {
        LinearDisplayP3.into()
    }

    fn pcs(&self) -> Pcs {
        Pcs::XyzD50
    }

    fn space_to_pcs(
        &self,
        intent: IccRenderingIntent,
    ) -> Result<ColorTransform, ProfileConnectionError> {
        require_relative(intent)?;
        Ok(ColorTransform::rgb_to_pcs_from(
            self.space(),
            rgb_to_xyz_d65_transfer(LinearDisplayP3::TO_XYZ_D65, TransferCurve::IDENTITY)?,
            self.pcs(),
        )?)
    }

    fn pcs_to_space(
        &self,
        intent: IccRenderingIntent,
    ) -> Result<ColorTransform, ProfileConnectionError> {
        require_relative(intent)?;
        Ok(ColorTransform::pcs_to_rgb_from(
            self.pcs(),
            self.space(),
            xyz_d65_to_rgb_transfer(LinearDisplayP3::FROM_XYZ_D65, TransferCurve::IDENTITY)?,
        )?)
    }

    fn direct_to(
        &self,
        destination: &ColorProfile,
        intent: IccRenderingIntent,
    ) -> Result<Option<ColorTransform>, ProfileConnectionError> {
        rgb_direct_to(
            LinearDisplayP3.into(),
            DisplayP3.into(),
            TransferCurve::SRGB_ENCODE,
            destination,
            intent,
        )
    }
}
