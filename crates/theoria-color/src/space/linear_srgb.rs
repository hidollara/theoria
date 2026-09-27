use super::{
    ColorProfile, ColorSpace, ColorTransform, IccRenderingIntent, Pcs, ProfileConnectionError,
    ProfileDefinition, Srgb, TransferCurve, TypedColorSpace, require_relative, rgb_direct_to,
    rgb_to_xyz_d65_transfer, xyz_d65_to_rgb_transfer,
};
use crate::model::Rgb;

named_space!(LinearSrgb, Rgb);

impl LinearSrgb {
    pub(crate) const TO_XYZ_D65: [[f64; 3]; 3] = Srgb::TO_XYZ_D65;
    pub(crate) const FROM_XYZ_D65: [[f64; 3]; 3] = Srgb::FROM_XYZ_D65;
}

predefined_space!(LinearSrgb, LinearSrgbProfile, LINEAR_SRGB_KEY);

impl ProfileDefinition for LinearSrgbProfile {
    fn space(&self) -> ColorSpace {
        LinearSrgb.into()
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
            rgb_to_xyz_d65_transfer(LinearSrgb::TO_XYZ_D65, TransferCurve::IDENTITY)?,
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
            xyz_d65_to_rgb_transfer(LinearSrgb::FROM_XYZ_D65, TransferCurve::IDENTITY)?,
        )?)
    }

    fn direct_to(
        &self,
        destination: &ColorProfile,
        intent: IccRenderingIntent,
    ) -> Result<Option<ColorTransform>, ProfileConnectionError> {
        rgb_direct_to(
            LinearSrgb.into(),
            Srgb.into(),
            TransferCurve::SRGB_ENCODE,
            destination,
            intent,
        )
    }
}
