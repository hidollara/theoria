use super::{
    ColorProfile, ColorSpace, ConnectionSpace, DisplayP3, SpaceConnectionError, TypedColorSpace,
    XyzD65,
};
use crate::model::Rgb;
use crate::transfer::{Transfer, TransferCurve, TransferError, TransferMatrix, TransferStage};

named_space!(LinearDisplayP3, Rgb);

impl LinearDisplayP3 {
    pub(crate) fn transfer_to_xyz_d65() -> Result<Transfer, TransferError> {
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

    fn connection_space(&self) -> ConnectionSpace {
        ConnectionSpace::XyzD65
    }

    fn transfer_to_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(LinearDisplayP3::transfer_to_xyz_d65()?)
    }

    fn transfer_from_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(XyzD65::transfer_to_linear_display_p3()?)
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
