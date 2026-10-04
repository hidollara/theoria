use super::{
    ColorProfile, ColorSpace, ConnectionSpace, HslSrgb, HwbSrgb, SpaceConnectionError, Srgb,
    TypedColorSpace,
};
use crate::model::Rgb;
use crate::transfer::{Transfer, TransferCurve, TransferError};

named_space!(LinearSrgb, Rgb);

impl LinearSrgb {
    pub(crate) fn transfer_to_srgb() -> Result<Transfer, TransferError> {
        Transfer::curves(vec![TransferCurve::SRGB_ENCODE; 3])
    }

    // Exact rational matrix from CSS Color 4's color-conversion sample code.
    const TO_XYZ_D65: [[f64; 3]; 3] = [
        [506752.0 / 1228815.0, 87881.0 / 245763.0, 12673.0 / 70218.0],
        [87098.0 / 409605.0, 175762.0 / 245763.0, 12673.0 / 175545.0],
        [7918.0 / 409605.0, 87881.0 / 737289.0, 1001167.0 / 1053270.0],
    ];

    // Exact rational inverse matrix from CSS Color 4's color-conversion sample code.
    const FROM_XYZ_D65: [[f64; 3]; 3] = [
        [12831.0 / 3959.0, -329.0 / 214.0, -1974.0 / 3959.0],
        [
            -851781.0 / 878810.0,
            1648619.0 / 878810.0,
            36519.0 / 878810.0,
        ],
        [705.0 / 12673.0, -2585.0 / 12673.0, 705.0 / 667.0],
    ];

    pub(crate) fn transfer_to_xyz_d65() -> Result<Transfer, TransferError> {
        Transfer::matrix(Self::TO_XYZ_D65, [0.0; 3])
    }

    pub(crate) fn transfer_from_xyz_d65() -> Result<Transfer, TransferError> {
        Transfer::matrix(Self::FROM_XYZ_D65, [0.0; 3])
    }
}

builtin_space!(LinearSrgb, LinearSrgbProfile);

impl ColorProfile for LinearSrgbProfile {
    fn model(&self) -> crate::model::ColorModel {
        <Rgb as crate::model::TypedColorModel>::MODEL
    }

    fn connection_space(&self) -> ConnectionSpace {
        ConnectionSpace::XyzD65
    }

    fn transfer_to_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(LinearSrgb::transfer_to_xyz_d65()?)
    }

    fn transfer_from_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(LinearSrgb::transfer_from_xyz_d65()?)
    }

    fn direct_transfer_to(
        &self,
        destination: &ColorSpace,
    ) -> Result<Option<Transfer>, SpaceConnectionError> {
        if *destination == ColorSpace::from(Srgb) {
            Ok(Some(LinearSrgb::transfer_to_srgb()?))
        } else if *destination == ColorSpace::from(HslSrgb) {
            Ok(Some(
                LinearSrgb::transfer_to_srgb()?.then(&HslSrgb::transfer_from_srgb()?)?,
            ))
        } else if *destination == ColorSpace::from(HwbSrgb) {
            Ok(Some(
                LinearSrgb::transfer_to_srgb()?.then(&HwbSrgb::transfer_from_srgb()?)?,
            ))
        } else {
            Ok(None)
        }
    }
}
