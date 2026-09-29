use super::{
    ColorProfile, ColorSpace, ConnectionSpace, SpaceConnectionError, Srgb, TypedColorSpace, XyzD65,
};
use crate::model::Rgb;
use crate::transfer::{Transfer, TransferCurve, TransferError, TransferMatrix, TransferStage};

named_space!(LinearSrgb, Rgb);

impl LinearSrgb {
    pub(crate) fn transfer_to_xyz_d65() -> Result<Transfer, TransferError> {
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
    fn channels(&self) -> usize {
        3
    }

    fn connection_space(&self) -> ConnectionSpace {
        ConnectionSpace::XyzD65
    }

    fn transfer_to_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(LinearSrgb::transfer_to_xyz_d65()?)
    }

    fn transfer_from_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(XyzD65::transfer_to_linear_srgb()?)
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
        } else {
            Ok(None)
        }
    }
}
