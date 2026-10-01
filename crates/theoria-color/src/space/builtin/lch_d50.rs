use super::{
    ColorProfile, ColorSpace, ConnectionSpace, LabD50, SpaceConnectionError, TypedColorSpace,
    XyzD50,
};
use crate::model::Lch;
use crate::transfer::{ColorFunction, Transfer, TransferError, TransferStage};

named_space!(LchD50, Lch);

impl LchD50 {
    pub(crate) fn transfer_to_lab_d50() -> Result<Transfer, TransferError> {
        Transfer::new([TransferStage::ColorFunction(
            ColorFunction::PolarToCartesian,
        )])
    }

    pub(crate) fn transfer_from_lab_d50() -> Result<Transfer, TransferError> {
        Transfer::new([TransferStage::ColorFunction(
            ColorFunction::CartesianToPolar,
        )])
    }

    pub(crate) fn transfer_to_xyz_d50() -> Result<Transfer, TransferError> {
        Self::transfer_to_lab_d50()?.then(&LabD50::transfer_to_xyz_d50())
    }

    pub(crate) fn transfer_from_xyz_d50() -> Result<Transfer, TransferError> {
        XyzD50::transfer_to_lab_d50().then(&Self::transfer_from_lab_d50()?)
    }
}

builtin_space!(LchD50, LchD50Profile);

impl ColorProfile for LchD50Profile {
    fn channels(&self) -> usize {
        3
    }

    fn polar_hue_index(&self) -> Option<usize> {
        Some(2)
    }

    fn hue_is_powerless(&self, coordinates: &[f64]) -> bool {
        coordinates[1] <= 0.0015
    }

    fn connection_space(&self) -> ConnectionSpace {
        ConnectionSpace::XyzD50
    }

    fn transfer_to_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(LchD50::transfer_to_xyz_d50()?)
    }

    fn transfer_from_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(LchD50::transfer_from_xyz_d50()?)
    }

    fn direct_transfer_to(
        &self,
        destination: &ColorSpace,
    ) -> Result<Option<Transfer>, SpaceConnectionError> {
        if *destination == ColorSpace::from(LabD50) {
            Ok(Some(LchD50::transfer_to_lab_d50()?))
        } else {
            Ok(None)
        }
    }
}
