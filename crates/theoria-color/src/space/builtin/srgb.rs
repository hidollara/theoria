use super::{
    ColorProfile, ColorSpace, ConnectionSpace, SpaceConnectionError, TypedColorSpace, XyzD65,
};
use crate::model::Rgb;
use crate::transfer::{Transfer, TransferCurve, TransferError, TransferMatrix, TransferStage};

named_space!(Srgb, Rgb);

impl Srgb {
    // Exact rational matrices from CSS Color 4's color-conversion sample code.
    // The linear and encoded sRGB spaces share these primaries.
    pub(crate) const TO_XYZ_D65: [[f64; 3]; 3] = [
        [506752.0 / 1228815.0, 87881.0 / 245763.0, 12673.0 / 70218.0],
        [87098.0 / 409605.0, 175762.0 / 245763.0, 12673.0 / 175545.0],
        [7918.0 / 409605.0, 87881.0 / 737289.0, 1001167.0 / 1053270.0],
    ];

    pub(crate) fn transfer_to_xyz_d65() -> Result<Transfer, TransferError> {
        Transfer::new([
            TransferStage::curves(vec![TransferCurve::SRGB_DECODE; 3])?,
            TransferStage::Matrix(TransferMatrix::new(
                3,
                3,
                Self::TO_XYZ_D65.into_iter().flatten().collect::<Vec<_>>(),
                [0.0; 3],
            )?),
        ])
    }
}

builtin_space!(Srgb, SrgbProfile);

impl ColorProfile for SrgbProfile {
    fn channels(&self) -> usize {
        3
    }

    fn connection_space(&self) -> ConnectionSpace {
        ConnectionSpace::XyzD65
    }

    fn transfer_to_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(Srgb::transfer_to_xyz_d65()?)
    }

    fn transfer_from_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(XyzD65::transfer_to_srgb()?)
    }

    fn direct_transfer_to(
        &self,
        destination: &ColorSpace,
    ) -> Result<Option<Transfer>, SpaceConnectionError> {
        if *destination == ColorSpace::from(super::LinearSrgb) {
            Ok(Some(Transfer::new([TransferStage::curves(vec![
                TransferCurve::SRGB_DECODE;
                3
            ])?])?))
        } else {
            Ok(None)
        }
    }
}
