use super::{ColorProfile, ConnectionSpace, OklabD65, SpaceConnectionError, TypedColorSpace};
use crate::model::Oklch;
use crate::transfer::{ColorFunction, Transfer, TransferError, TransferStage};

named_space!(OklchD65, Oklch);

impl OklchD65 {
    pub(crate) fn transfer_to_xyz_d65() -> Result<Transfer, TransferError> {
        Transfer::new([TransferStage::ColorFunction(
            ColorFunction::PolarToCartesian,
        )])?
        .then(&OklabD65::transfer_to_xyz_d65()?)
    }

    pub(crate) fn transfer_from_xyz_d65() -> Result<Transfer, TransferError> {
        OklabD65::transfer_from_xyz_d65()?.then(&Transfer::new([TransferStage::ColorFunction(
            ColorFunction::CartesianToPolar,
        )])?)
    }
}

builtin_space!(OklchD65, OklchD65Profile);

impl ColorProfile for OklchD65Profile {
    fn channels(&self) -> usize {
        3
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
}
