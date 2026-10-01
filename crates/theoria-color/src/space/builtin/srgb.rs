use super::{
    ColorProfile, ColorSpace, ConnectionSpace, HslSrgb, HwbSrgb, LinearSrgb, SpaceConnectionError,
    TypedColorSpace,
};
use crate::model::Rgb;
use crate::transfer::{Transfer, TransferCurve, TransferError};

named_space!(Srgb, Rgb);

impl Srgb {
    pub(crate) fn transfer_to_linear_srgb() -> Result<Transfer, TransferError> {
        Transfer::curves(vec![TransferCurve::SRGB_DECODE; 3])
    }

    pub(crate) fn transfer_to_xyz_d65() -> Result<Transfer, TransferError> {
        Self::transfer_to_linear_srgb()?.then(&LinearSrgb::transfer_to_xyz_d65()?)
    }

    pub(crate) fn transfer_from_xyz_d65() -> Result<Transfer, TransferError> {
        LinearSrgb::transfer_from_xyz_d65()?.then(&LinearSrgb::transfer_to_srgb()?)
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
        Ok(Srgb::transfer_from_xyz_d65()?)
    }

    fn direct_transfer_to(
        &self,
        destination: &ColorSpace,
    ) -> Result<Option<Transfer>, SpaceConnectionError> {
        if *destination == ColorSpace::from(super::LinearSrgb) {
            Ok(Some(Srgb::transfer_to_linear_srgb()?))
        } else if *destination == ColorSpace::from(HslSrgb) {
            Ok(Some(HslSrgb::transfer_from_srgb()?))
        } else if *destination == ColorSpace::from(HwbSrgb) {
            Ok(Some(HwbSrgb::transfer_from_srgb()?))
        } else {
            Ok(None)
        }
    }
}
