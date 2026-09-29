use super::{ColorProfile, ColorSpace, Pcs, SpaceConnectionError, TypedColorSpace, XyzD50, XyzD65};
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

    pub(crate) fn to_xyz_d65_transfer() -> Result<Transfer, TransferError> {
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
    fn model(&self) -> crate::model::ColorModel {
        crate::model::ColorModel::Rgb
    }

    fn pcs(&self) -> Pcs {
        Pcs::XyzD50
    }

    fn space_to_pcs_transfer(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(Srgb::to_xyz_d65_transfer()?.then(&XyzD65::to_xyz_d50_transfer())?)
    }

    fn pcs_to_space_transfer(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(XyzD50::to_xyz_d65_transfer().then(&XyzD65::to_srgb_transfer()?)?)
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
        } else if *destination == ColorSpace::from(XyzD65) {
            Ok(Some(Srgb::to_xyz_d65_transfer()?))
        } else if *destination == ColorSpace::from(XyzD50) {
            Ok(Some(self.space_to_pcs_transfer()?))
        } else {
            Ok(None)
        }
    }
}
