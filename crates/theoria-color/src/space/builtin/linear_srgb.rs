use super::{
    ColorProfile, ColorSpace, Pcs, SpaceConnectionError, Srgb, Transfer, TransferCurve,
    TypedColorSpace, pcs_to_rgb_transfer, rgb_direct_to, rgb_to_pcs_transfer,
    rgb_to_xyz_d65_transfer, xyz_d65_to_rgb_transfer,
};
use crate::model::Rgb;

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
        Ok(rgb_to_pcs_transfer(rgb_to_xyz_d65_transfer(
            LinearSrgb::TO_XYZ_D65,
            TransferCurve::IDENTITY,
        )?)?)
    }

    fn pcs_to_space_transfer(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(pcs_to_rgb_transfer(xyz_d65_to_rgb_transfer(
            LinearSrgb::FROM_XYZ_D65,
            TransferCurve::IDENTITY,
        )?)?)
    }

    fn direct_transfer_to(
        &self,
        destination: &ColorSpace,
    ) -> Result<Option<Transfer>, SpaceConnectionError> {
        rgb_direct_to(Srgb.into(), TransferCurve::SRGB_ENCODE, destination)
    }
}
