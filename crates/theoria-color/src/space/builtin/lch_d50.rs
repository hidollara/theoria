use super::calculation::{input, number, transfer};
use super::{ColorSpace, ConnectionSpace, LabD50, SpaceConnectionError, TypedColorSpace};
use crate::model::Lch;
use crate::transfer::{CalculatorBinary as B, CalculatorUnary as U, Transfer, TransferError};

builtin_space!(LchD50);

impl LchD50 {
    pub(crate) fn transfer_to_lab_d50() -> Result<Transfer, TransferError> {
        Self::polar_to_cartesian()
    }

    pub(crate) fn transfer_from_lab_d50() -> Result<Transfer, TransferError> {
        Self::cartesian_to_polar()
    }

    pub(super) fn polar_to_cartesian() -> Result<Transfer, TransferError> {
        let angle = input(2) * number(std::f64::consts::PI / 180.0);
        transfer([
            input(0),
            input(1) * angle.clone().unary(U::Cos),
            input(1) * angle.unary(U::Sin),
        ])
    }

    pub(super) fn cartesian_to_polar() -> Result<Transfer, TransferError> {
        // Scaled hypot avoids squaring the unscaled coordinates.
        let x = input(1).unary(U::Abs);
        let y = input(2).unary(U::Abs);
        let scale = x.clone().max(y.clone());
        let a = x / scale.clone();
        let b = y / scale.clone();
        let chroma = scale.clone().eq(number(0.0)).choose(
            number(0.0),
            scale.clone() * (a.clone() * a + b.clone() * b).unary(U::SquareRoot),
        );
        let hue = scale.eq(number(0.0)).choose(
            number(0.0),
            (input(1).binary(input(2), B::Atan2) * number(180.0 / std::f64::consts::PI))
                .modulo(360.0),
        );
        transfer([input(0), chroma, hue])
    }

    pub(crate) fn transfer_to_xyz_d50() -> Result<Transfer, TransferError> {
        Self::transfer_to_lab_d50()?.then(&LabD50::transfer_to_xyz_d50())
    }

    pub(crate) fn transfer_from_xyz_d50() -> Result<Transfer, TransferError> {
        LabD50::transfer_from_xyz_d50().then(&Self::transfer_from_lab_d50()?)
    }
}

impl TypedColorSpace for LchD50 {
    type Model = Lch;

    fn polar_hue_index() -> Option<usize> {
        Some(2)
    }

    fn hue_is_powerless(coordinates: &[f64]) -> bool {
        coordinates[1] <= 0.0015
    }

    fn connection_space() -> ConnectionSpace {
        ConnectionSpace::XyzD50
    }

    fn transfer_to_connection_space() -> Result<Transfer, SpaceConnectionError> {
        Ok(LchD50::transfer_to_xyz_d50()?)
    }

    fn transfer_from_connection_space() -> Result<Transfer, SpaceConnectionError> {
        Ok(LchD50::transfer_from_xyz_d50()?)
    }

    fn direct_transfer_to(
        destination: &ColorSpace,
    ) -> Result<Option<Transfer>, SpaceConnectionError> {
        if *destination == ColorSpace::from(LabD50) {
            Ok(Some(LchD50::transfer_to_lab_d50()?))
        } else {
            Ok(None)
        }
    }
}
