use super::{
    ColorProfile, ColorSpace, DisplayP3, Pcs, SpaceConnectionError, Transfer, TransferCurve,
    TypedColorSpace, pcs_to_rgb_transfer, rgb_direct_to, rgb_to_pcs_transfer,
    rgb_to_xyz_d65_transfer, xyz_d65_to_rgb_transfer,
};
use crate::model::Rgb;

named_space!(LinearDisplayP3, Rgb);

impl LinearDisplayP3 {
    pub(crate) const TO_XYZ_D65: [[f64; 3]; 3] = DisplayP3::TO_XYZ_D65;
    pub(crate) const FROM_XYZ_D65: [[f64; 3]; 3] = DisplayP3::FROM_XYZ_D65;
}

builtin_space!(LinearDisplayP3, LinearDisplayP3Profile);

impl ColorProfile for LinearDisplayP3Profile {
    fn model(&self) -> crate::model::ColorModel {
        crate::model::ColorModel::Rgb
    }

    fn pcs(&self) -> Pcs {
        Pcs::XyzD50
    }

    fn space_to_pcs_transfer(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(rgb_to_pcs_transfer(rgb_to_xyz_d65_transfer(
            LinearDisplayP3::TO_XYZ_D65,
            TransferCurve::IDENTITY,
        )?)?)
    }

    fn pcs_to_space_transfer(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(pcs_to_rgb_transfer(xyz_d65_to_rgb_transfer(
            LinearDisplayP3::FROM_XYZ_D65,
            TransferCurve::IDENTITY,
        )?)?)
    }

    fn direct_transfer_to(
        &self,
        destination: &ColorSpace,
    ) -> Result<Option<Transfer>, SpaceConnectionError> {
        rgb_direct_to(DisplayP3.into(), TransferCurve::SRGB_ENCODE, destination)
    }
}
