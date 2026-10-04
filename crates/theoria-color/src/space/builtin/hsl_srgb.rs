use super::calculation::{Expression, input, number, transfer};
use super::{
    ColorProfile, ColorSpace, ConnectionSpace, HwbSrgb, LinearSrgb, SpaceConnectionError, Srgb,
    TypedColorSpace,
};
use crate::model::Hls;
use crate::transfer::{Transfer, TransferError};

// The named HSL space exposes canonical HLS-ordered coordinates.
named_space!(HslSrgb, Hls);

impl HslSrgb {
    pub(crate) fn transfer_to_srgb() -> Result<Transfer, TransferError> {
        transfer(Self::rgb_channels(
            input(0),
            input(1) / number(100.0),
            input(2) / number(100.0),
        ))
    }

    pub(crate) fn transfer_from_srgb() -> Result<Transfer, TransferError> {
        let max = input(0).max(input(1)).max(input(2));
        let min = input(0).min(input(1)).min(input(2));
        let light = (min.clone() + max.clone()) / number(2.0);
        let saturation = max.clone().eq(min).choose(
            number(0.0),
            light.clone().eq(number(0.0)).choose(
                number(0.0),
                light.clone().eq(number(1.0)).choose(
                    number(0.0),
                    (max - light.clone()) / light.clone().min(number(1.0) - light.clone()),
                ),
            ),
        );
        let hue = Self::hue();
        transfer([
            saturation
                .clone()
                .lt(number(0.0))
                .choose((hue.clone() + number(180.0)).modulo(360.0), hue),
            light * number(100.0),
            saturation
                .clone()
                .lt(number(0.0))
                .choose(number(0.0) - saturation.clone(), saturation)
                * number(100.0),
        ])
    }

    pub(super) fn hue() -> Expression {
        let max = input(0).max(input(1)).max(input(2));
        let min = input(0).min(input(1)).min(input(2));
        let delta = max.clone() - min;
        let sector = max.clone().eq(input(0)).choose(
            (input(1) - input(2)) / delta.clone(),
            max.eq(input(1)).choose(
                (input(2) - input(0)) / delta.clone() + number(2.0),
                (input(0) - input(1)) / delta.clone() + number(4.0),
            ),
        );
        delta
            .eq(number(0.0))
            .choose(number(0.0), (sector * number(60.0)).modulo(360.0))
    }

    pub(super) fn rgb_channels(
        hue: Expression,
        light: Expression,
        saturation: Expression,
    ) -> [Expression; 3] {
        let amplitude = saturation * light.clone().min(number(1.0) - light.clone());
        [0.0, 8.0, 4.0].map(|offset| {
            let k = (number(offset) + hue.clone() / number(30.0)).modulo(12.0);
            light.clone()
                - amplitude.clone()
                    * number(-1.0).max(
                        (k.clone() - number(3.0))
                            .min(number(9.0) - k)
                            .min(number(1.0)),
                    )
        })
    }

    pub(crate) fn transfer_to_xyz_d65() -> Result<Transfer, TransferError> {
        Self::transfer_to_srgb()?.then(&Srgb::transfer_to_xyz_d65()?)
    }

    pub(crate) fn transfer_from_xyz_d65() -> Result<Transfer, TransferError> {
        Srgb::transfer_from_xyz_d65()?.then(&Self::transfer_from_srgb()?)
    }
}

builtin_space!(HslSrgb, HslSrgbProfile);

impl ColorProfile for HslSrgbProfile {
    fn model(&self) -> crate::model::ColorModel {
        <Hls as crate::model::TypedColorModel>::MODEL
    }

    fn polar_hue_index(&self) -> Option<usize> {
        Some(0)
    }

    fn hue_is_powerless(&self, coordinates: &[f64]) -> bool {
        coordinates[2] <= 0.0
    }

    fn connection_space(&self) -> ConnectionSpace {
        ConnectionSpace::XyzD65
    }

    fn transfer_to_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(HslSrgb::transfer_to_xyz_d65()?)
    }

    fn transfer_from_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(HslSrgb::transfer_from_xyz_d65()?)
    }

    fn direct_transfer_to(
        &self,
        destination: &ColorSpace,
    ) -> Result<Option<Transfer>, SpaceConnectionError> {
        if *destination == ColorSpace::from(Srgb) {
            Ok(Some(HslSrgb::transfer_to_srgb()?))
        } else if *destination == ColorSpace::from(LinearSrgb) {
            Ok(Some(
                HslSrgb::transfer_to_srgb()?.then(&Srgb::transfer_to_linear_srgb()?)?,
            ))
        } else if *destination == ColorSpace::from(HwbSrgb) {
            Ok(Some(
                HslSrgb::transfer_to_srgb()?.then(&HwbSrgb::transfer_from_srgb()?)?,
            ))
        } else {
            Ok(None)
        }
    }
}
