use super::LchD50;
use super::{
    ColorProfile, ColorSpace, ConnectionSpace, OklabD65, SpaceConnectionError, TypedColorSpace,
};
use crate::model::Lch;
use crate::transfer::{Transfer, TransferError};

named_space!(OklchD65, Lch);

impl OklchD65 {
    pub(crate) fn transfer_to_oklab_d65() -> Result<Transfer, TransferError> {
        LchD50::polar_to_cartesian()
    }

    pub(crate) fn transfer_from_oklab_d65() -> Result<Transfer, TransferError> {
        LchD50::cartesian_to_polar()
    }

    pub(crate) fn transfer_to_xyz_d65() -> Result<Transfer, TransferError> {
        Self::transfer_to_oklab_d65()?.then(&OklabD65::transfer_to_xyz_d65()?)
    }

    pub(crate) fn transfer_from_xyz_d65() -> Result<Transfer, TransferError> {
        OklabD65::transfer_from_xyz_d65()?.then(&Self::transfer_from_oklab_d65()?)
    }
}

builtin_space!(OklchD65, OklchD65Profile);

impl ColorProfile for OklchD65Profile {
    fn model(&self) -> crate::model::ColorModel {
        <Lch as crate::model::TypedColorModel>::MODEL
    }

    fn polar_hue_index(&self) -> Option<usize> {
        Some(2)
    }

    fn hue_is_powerless(&self, coordinates: &[f64]) -> bool {
        coordinates[1] <= 0.000004
    }

    fn connection_space(&self) -> ConnectionSpace {
        ConnectionSpace::XyzD65
    }

    fn transfer_to_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(OklchD65::transfer_to_xyz_d65()?)
    }

    fn transfer_from_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(OklchD65::transfer_from_xyz_d65()?)
    }

    fn direct_transfer_to(
        &self,
        destination: &ColorSpace,
    ) -> Result<Option<Transfer>, SpaceConnectionError> {
        if *destination == ColorSpace::from(OklabD65) {
            Ok(Some(OklchD65::transfer_to_oklab_d65()?))
        } else {
            Ok(None)
        }
    }
}
