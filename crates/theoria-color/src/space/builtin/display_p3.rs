use super::{
    ColorProfile, ColorSpace, ConnectionSpace, LinearDisplayP3, LinearSrgb, SpaceConnectionError,
    Srgb, TypedColorSpace,
};
use crate::model::Rgb;
use crate::transfer::{Transfer, TransferError};

named_space!(DisplayP3, Rgb);

impl DisplayP3 {
    pub(crate) fn transfer_to_xyz_d65() -> Result<Transfer, TransferError> {
        Transfer::curves(vec![Srgb::decode_curve().clone(); 3])?
            .then(&LinearDisplayP3::transfer_to_xyz_d65()?)
    }

    pub(crate) fn transfer_from_xyz_d65() -> Result<Transfer, TransferError> {
        LinearDisplayP3::transfer_from_xyz_d65()?.then(&Transfer::curves(
            vec![LinearSrgb::encode_curve().clone(); 3],
        )?)
    }
}

builtin_space!(DisplayP3, DisplayP3Profile);

impl ColorProfile for DisplayP3Profile {
    fn model(&self) -> crate::model::ColorModel {
        <Rgb as crate::model::TypedColorModel>::MODEL
    }

    fn connection_space(&self) -> ConnectionSpace {
        ConnectionSpace::XyzD65
    }

    fn transfer_to_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(DisplayP3::transfer_to_xyz_d65()?)
    }

    fn transfer_from_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(DisplayP3::transfer_from_xyz_d65()?)
    }

    fn direct_transfer_to(
        &self,
        destination: &ColorSpace,
    ) -> Result<Option<Transfer>, SpaceConnectionError> {
        if *destination == ColorSpace::from(super::LinearDisplayP3) {
            Ok(Some(Transfer::curves(vec![
                Srgb::decode_curve().clone();
                3
            ])?))
        } else {
            Ok(None)
        }
    }
}
