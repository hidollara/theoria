use super::calculation::{input, number, transfer};
use super::{
    ColorProfile, ColorSpace, ConnectionSpace, HslSrgb, LinearSrgb, SpaceConnectionError, Srgb,
    TypedColorSpace,
};
use crate::model::Hwb;
use crate::transfer::{Transfer, TransferError};

named_space!(HwbSrgb, Hwb);

impl HwbSrgb {
    pub(crate) fn transfer_to_srgb() -> Result<Transfer, TransferError> {
        let white = input(1) / number(100.0);
        let black = input(2) / number(100.0);
        let sum = white.clone() + black;
        transfer(
            HslSrgb::rgb_channels(input(0), number(0.5), number(1.0)).map(|channel| {
                sum.clone().ge(number(1.0)).choose(
                    white.clone() / sum.clone(),
                    channel * (number(1.0) - sum.clone()) + white.clone(),
                )
            }),
        )
    }

    pub(crate) fn transfer_from_srgb() -> Result<Transfer, TransferError> {
        transfer([
            HslSrgb::hue(),
            input(0).min(input(1)).min(input(2)) * number(100.0),
            (number(1.0) - input(0).max(input(1)).max(input(2))) * number(100.0),
        ])
    }

    pub(crate) fn transfer_to_xyz_d65() -> Result<Transfer, TransferError> {
        Self::transfer_to_srgb()?.then(&Srgb::transfer_to_xyz_d65()?)
    }

    pub(crate) fn transfer_from_xyz_d65() -> Result<Transfer, TransferError> {
        Srgb::transfer_from_xyz_d65()?.then(&Self::transfer_from_srgb()?)
    }
}

builtin_space!(HwbSrgb, HwbSrgbProfile);

impl ColorProfile for HwbSrgbProfile {
    fn model(&self) -> crate::model::ColorModel {
        <Hwb as crate::model::TypedColorModel>::MODEL
    }

    fn polar_hue_index(&self) -> Option<usize> {
        Some(0)
    }

    fn hue_is_powerless(&self, coordinates: &[f64]) -> bool {
        coordinates[1] + coordinates[2] >= 100.0
    }

    fn connection_space(&self) -> ConnectionSpace {
        ConnectionSpace::XyzD65
    }

    fn transfer_to_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(HwbSrgb::transfer_to_xyz_d65()?)
    }

    fn transfer_from_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(HwbSrgb::transfer_from_xyz_d65()?)
    }

    fn direct_transfer_to(
        &self,
        destination: &ColorSpace,
    ) -> Result<Option<Transfer>, SpaceConnectionError> {
        if *destination == ColorSpace::from(Srgb) {
            Ok(Some(HwbSrgb::transfer_to_srgb()?))
        } else if *destination == ColorSpace::from(LinearSrgb) {
            Ok(Some(
                HwbSrgb::transfer_to_srgb()?.then(&Srgb::transfer_to_linear_srgb()?)?,
            ))
        } else if *destination == ColorSpace::from(HslSrgb) {
            Ok(Some(
                HwbSrgb::transfer_to_srgb()?.then(&HslSrgb::transfer_from_srgb()?)?,
            ))
        } else {
            Ok(None)
        }
    }
}
