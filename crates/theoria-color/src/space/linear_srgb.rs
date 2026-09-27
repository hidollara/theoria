use super::{
    ColorSpace, IccRenderingIntent, Pcs, ProfileConnectionError, ProfileDefinition, Srgb, Transfer,
    TransferCurve, TypedColorSpace, pcs_to_rgb_transfer, require_relative, rgb_direct_to,
    rgb_to_pcs_transfer, rgb_to_xyz_d65_transfer, xyz_d65_to_rgb_transfer,
};
use crate::model::Rgb;

named_space!(LinearSrgb, Rgb);

impl LinearSrgb {
    pub(crate) const TO_XYZ_D65: [[f64; 3]; 3] = Srgb::TO_XYZ_D65;
    pub(crate) const FROM_XYZ_D65: [[f64; 3]; 3] = Srgb::FROM_XYZ_D65;
}

predefined_space!(LinearSrgb, LinearSrgbProfile);

impl ProfileDefinition for LinearSrgbProfile {
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
            rgb_to_xyz_d65_transfer(LinearSrgb::TO_XYZ_D65, TransferCurve::IDENTITY)?,
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
            xyz_d65_to_rgb_transfer(LinearSrgb::FROM_XYZ_D65, TransferCurve::IDENTITY)?,
        )?)
    }

    fn direct_transfer_to(
        &self,
        destination: &ColorSpace,
        intent: IccRenderingIntent,
    ) -> Result<Option<Transfer>, ProfileConnectionError> {
        rgb_direct_to(
            LinearSrgb.into(),
            Srgb.into(),
            TransferCurve::SRGB_ENCODE,
            destination,
            intent,
        )
    }
}
