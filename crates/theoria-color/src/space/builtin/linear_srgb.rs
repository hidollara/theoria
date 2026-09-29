use super::{
    ColorProfile, ColorSpace, Pcs, SpaceConnectionError, Srgb, Transfer, TransferCurve,
    TypedColorSpace, XyzD50, XyzD65, rgb_to_xyz_d65_transfer, xyz_d65_to_rgb_transfer,
};
use crate::model::Rgb;
use crate::transfer::TransferStage;

named_space!(LinearSrgb, Rgb);

impl LinearSrgb {
    pub(crate) const TO_XYZ_D65: [[f64; 3]; 3] = Srgb::TO_XYZ_D65;
    pub(crate) const FROM_XYZ_D65: [[f64; 3]; 3] = Srgb::FROM_XYZ_D65;
}

builtin_space!(LinearSrgb, LinearSrgbProfile);

impl ColorProfile for LinearSrgbProfile {
    fn model(&self) -> crate::model::ColorModel {
        crate::model::ColorModel::Rgb
    }

    fn pcs(&self) -> Pcs {
        Pcs::XyzD50
    }

    fn space_to_pcs_transfer(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(
            rgb_to_xyz_d65_transfer(LinearSrgb::TO_XYZ_D65, TransferCurve::IDENTITY)?
                .then(&XyzD65::to_xyz_d50_transfer())?,
        )
    }

    fn pcs_to_space_transfer(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(XyzD50::to_xyz_d65_transfer().then(&xyz_d65_to_rgb_transfer(
            LinearSrgb::FROM_XYZ_D65,
            TransferCurve::IDENTITY,
        )?)?)
    }

    fn direct_transfer_to(
        &self,
        destination: &ColorSpace,
    ) -> Result<Option<Transfer>, SpaceConnectionError> {
        if *destination != ColorSpace::from(Srgb) {
            return Ok(None);
        }
        Ok(Some(Transfer::new([TransferStage::curves(vec![
            TransferCurve::SRGB_ENCODE;
            3
        ])?])?))
    }
}
