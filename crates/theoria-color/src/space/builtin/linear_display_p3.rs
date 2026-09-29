use super::{
    ColorProfile, ColorSpace, DisplayP3, Pcs, SpaceConnectionError, TypedColorSpace, XyzD50, XyzD65,
};
use crate::model::Rgb;
use crate::transfer::{Transfer, TransferCurve, TransferError, TransferMatrix, TransferStage};

named_space!(LinearDisplayP3, Rgb);

impl LinearDisplayP3 {
    pub(crate) fn to_xyz_d65_transfer() -> Result<Transfer, TransferError> {
        Transfer::new([TransferStage::Matrix(TransferMatrix::new(
            3,
            3,
            DisplayP3::TO_XYZ_D65
                .into_iter()
                .flatten()
                .collect::<Vec<_>>(),
            [0.0; 3],
        )?)])
    }
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
        Ok(LinearDisplayP3::to_xyz_d65_transfer()?.then(&XyzD65::to_xyz_d50_transfer())?)
    }

    fn pcs_to_space_transfer(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(XyzD50::to_xyz_d65_transfer().then(&XyzD65::to_linear_display_p3_transfer()?)?)
    }

    fn direct_transfer_to(
        &self,
        destination: &ColorSpace,
    ) -> Result<Option<Transfer>, SpaceConnectionError> {
        if *destination == ColorSpace::from(DisplayP3) {
            Ok(Some(Transfer::new([TransferStage::curves(vec![
                TransferCurve::SRGB_ENCODE;
                3
            ])?])?))
        } else {
            Ok(None)
        }
    }
}
