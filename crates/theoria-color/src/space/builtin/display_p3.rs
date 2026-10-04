use super::{
    ColorSpace, ConnectionSpace, LinearDisplayP3, SpaceConnectionError, Srgb, TypedColorSpace,
};
use crate::model::Rgb;
use crate::transfer::{Transfer, TransferError};

builtin_space!(DisplayP3);

impl DisplayP3 {
    pub(crate) fn transfer_to_linear_display_p3() -> Result<Transfer, TransferError> {
        Transfer::curves(vec![Srgb::decode_curve().clone(); 3])
    }

    pub(crate) fn transfer_from_linear_display_p3() -> Result<Transfer, TransferError> {
        Transfer::curves(vec![Srgb::encode_curve().clone(); 3])
    }

    pub(crate) fn transfer_to_xyz_d65() -> Result<Transfer, TransferError> {
        Self::transfer_to_linear_display_p3()?.then(&LinearDisplayP3::transfer_to_xyz_d65()?)
    }

    pub(crate) fn transfer_from_xyz_d65() -> Result<Transfer, TransferError> {
        LinearDisplayP3::transfer_from_xyz_d65()?.then(&Self::transfer_from_linear_display_p3()?)
    }
}

impl TypedColorSpace for DisplayP3 {
    type Model = Rgb;

    fn connection_space() -> ConnectionSpace {
        ConnectionSpace::XyzD65
    }

    fn transfer_to_connection_space() -> Result<Transfer, SpaceConnectionError> {
        Ok(DisplayP3::transfer_to_xyz_d65()?)
    }

    fn transfer_from_connection_space() -> Result<Transfer, SpaceConnectionError> {
        Ok(DisplayP3::transfer_from_xyz_d65()?)
    }

    fn direct_transfer_to(
        destination: &ColorSpace,
    ) -> Result<Option<Transfer>, SpaceConnectionError> {
        if *destination == ColorSpace::from(super::LinearDisplayP3) {
            Ok(Some(DisplayP3::transfer_to_linear_display_p3()?))
        } else {
            Ok(None)
        }
    }
}
