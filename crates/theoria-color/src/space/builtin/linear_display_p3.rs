use super::{
    ColorProfile, ColorSpace, DisplayP3, Pcs, SpaceConnectionError, Transfer, TransferCurve,
    TypedColorSpace, XyzD50, XyzD65, rgb_to_xyz_d65_transfer, xyz_d65_to_rgb_transfer,
};
use crate::model::Rgb;
use crate::transfer::TransferStage;

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
        Ok(
            rgb_to_xyz_d65_transfer(LinearDisplayP3::TO_XYZ_D65, TransferCurve::IDENTITY)?
                .then(&XyzD65::to_xyz_d50_transfer())?,
        )
    }

    fn pcs_to_space_transfer(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(XyzD50::to_xyz_d65_transfer().then(&xyz_d65_to_rgb_transfer(
            LinearDisplayP3::FROM_XYZ_D65,
            TransferCurve::IDENTITY,
        )?)?)
    }

    fn direct_transfer_to(
        &self,
        destination: &ColorSpace,
    ) -> Result<Option<Transfer>, SpaceConnectionError> {
        if *destination != ColorSpace::from(DisplayP3) {
            return Ok(None);
        }
        Ok(Some(Transfer::new([TransferStage::curves(vec![
            TransferCurve::SRGB_ENCODE;
            3
        ])?])?))
    }
}
