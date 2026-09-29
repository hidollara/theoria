use super::{ColorProfile, ColorSpace, Pcs, SpaceConnectionError, TypedColorSpace, XyzD50, XyzD65};
use crate::model::Rgb;
use crate::transfer::{Transfer, TransferCurve, TransferError, TransferMatrix, TransferStage};

named_space!(DisplayP3, Rgb);

impl DisplayP3 {
    // Exact rational matrices from CSS Color 4's color-conversion sample code.
    // The linear and encoded Display P3 spaces share these primaries.
    pub(crate) const TO_XYZ_D65: [[f64; 3]; 3] = [
        [
            608311.0 / 1250200.0,
            189793.0 / 714400.0,
            198249.0 / 1000160.0,
        ],
        [
            35783.0 / 156275.0,
            247089.0 / 357200.0,
            198249.0 / 2500400.0,
        ],
        [0.0, 32229.0 / 714400.0, 5220557.0 / 5000800.0],
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

builtin_space!(DisplayP3, DisplayP3Profile);

impl ColorProfile for DisplayP3Profile {
    fn model(&self) -> crate::model::ColorModel {
        crate::model::ColorModel::Rgb
    }

    fn pcs(&self) -> Pcs {
        Pcs::XyzD50
    }

    fn space_to_pcs_transfer(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(DisplayP3::to_xyz_d65_transfer()?.then(&XyzD65::to_xyz_d50_transfer())?)
    }

    fn pcs_to_space_transfer(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(XyzD50::to_xyz_d65_transfer().then(&XyzD65::to_display_p3_transfer()?)?)
    }

    fn direct_transfer_to(
        &self,
        destination: &ColorSpace,
    ) -> Result<Option<Transfer>, SpaceConnectionError> {
        if *destination == ColorSpace::from(super::LinearDisplayP3) {
            Ok(Some(Transfer::new([TransferStage::curves(vec![
                TransferCurve::SRGB_DECODE;
                3
            ])?])?))
        } else {
            Ok(None)
        }
    }
}
