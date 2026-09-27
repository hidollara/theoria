use super::{
    ColorProfile, ColorSpace, ColorTransform, IccPcs, IccRenderingIntent, ProfileConnectionError,
    ProfileDefinition, RgbColorSpace, Srgb, TransferCurve, TypedColorSpace, require_relative,
    rgb_direct_to, rgb_to_xyz_d65_transfer, xyz_d65_to_rgb_transfer,
};
use crate::model::Rgb;

named_space!(LinearSrgb, Rgb);

impl LinearSrgb {
    pub(crate) const TO_XYZ_D65: [[f64; 3]; 3] = Srgb::TO_XYZ_D65;
    pub(crate) const FROM_XYZ_D65: [[f64; 3]; 3] = Srgb::FROM_XYZ_D65;
}

rgb_color_space!(LinearSrgb, LinearSrgb);
predefined_space!(LinearSrgb, LINEAR_SRGB_KEY);

impl ProfileDefinition for LinearSrgb {
    fn space(&self) -> ColorSpace {
        (*self).into()
    }

    fn pcs(&self) -> IccPcs {
        IccPcs::XyzD50
    }

    fn space_to_pcs(
        &self,
        intent: IccRenderingIntent,
    ) -> Result<ColorTransform, ProfileConnectionError> {
        require_relative(intent)?;
        Ok(ColorTransform::rgb_to_pcs_from(
            self.space(),
            rgb_to_xyz_d65_transfer(Self::TO_XYZ_D65, TransferCurve::IDENTITY)?,
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
            xyz_d65_to_rgb_transfer(Self::FROM_XYZ_D65, TransferCurve::IDENTITY)?,
        )?)
    }

    fn direct_to(
        &self,
        destination: &ColorProfile,
        intent: IccRenderingIntent,
    ) -> Result<Option<ColorTransform>, ProfileConnectionError> {
        rgb_direct_to(RgbColorSpace::LinearSrgb, destination, intent)
    }
}
