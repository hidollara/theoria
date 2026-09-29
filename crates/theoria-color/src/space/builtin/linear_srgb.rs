use super::{
    ColorProfile, ColorSpace, Pcs, SpaceConnectionError, Srgb, TypedColorSpace, XyzD50, XyzD65,
};
use crate::model::Rgb;
use crate::transfer::{Transfer, TransferCurve, TransferError, TransferMatrix, TransferStage};

named_space!(LinearSrgb, Rgb);

impl LinearSrgb {
    pub(crate) fn to_xyz_d65_transfer() -> Result<Transfer, TransferError> {
        Transfer::new([TransferStage::Matrix(TransferMatrix::new(
            3,
            3,
            Srgb::TO_XYZ_D65.into_iter().flatten().collect::<Vec<_>>(),
            [0.0; 3],
        )?)])
    }
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
        Ok(LinearSrgb::to_xyz_d65_transfer()?.then(&XyzD65::to_xyz_d50_transfer())?)
    }

    fn pcs_to_space_transfer(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(XyzD50::to_xyz_d65_transfer().then(&XyzD65::to_linear_srgb_transfer()?)?)
    }

    fn direct_transfer_to(
        &self,
        destination: &ColorSpace,
    ) -> Result<Option<Transfer>, SpaceConnectionError> {
        if *destination == ColorSpace::from(Srgb) {
            Ok(Some(Transfer::new([TransferStage::curves(vec![
                TransferCurve::SRGB_ENCODE;
                3
            ])?])?))
        } else if *destination == ColorSpace::from(XyzD65) {
            Ok(Some(LinearSrgb::to_xyz_d65_transfer()?))
        } else if *destination == ColorSpace::from(XyzD50) {
            Ok(Some(self.space_to_pcs_transfer()?))
        } else {
            Ok(None)
        }
    }
}
