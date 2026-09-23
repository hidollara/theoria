use super::{RgbColorSpace, RgbTransfer};

/// Converts straight RGB coordinates between the predefined Canvas spaces.
///
/// This operates on logical coordinates only. Alpha association and renderer
/// working precision are deliberately outside this boundary.
pub(crate) fn convert_rgb(rgb: [f32; 3], from: RgbColorSpace, to: RgbColorSpace) -> [f32; 3] {
    if from == to {
        rgb
    } else {
        let source = from.transform();
        let destination = to.transform();
        if source.to_xyz_d65_matrix() == destination.to_xyz_d65_matrix() {
            destination.encode(source.decode(rgb))
        } else {
            destination.xyz_d65_to_rgb(source.rgb_to_xyz_d65(rgb))
        }
    }
}

pub(crate) fn from_linear_srgb(rgb: [f32; 3], space: RgbColorSpace) -> [f32; 3] {
    convert_rgb(rgb, RgbColorSpace::LinearSrgb, space)
}

pub(crate) fn to_linear_srgb(rgb: [f32; 3], space: RgbColorSpace) -> [f32; 3] {
    convert_rgb(rgb, space, RgbColorSpace::LinearSrgb)
}

pub(crate) fn encode_srgb(value: f32) -> f32 {
    RgbTransfer::Srgb.encode(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn predefined_spaces_round_trip_through_linear_srgb() {
        let color = [0.23, 0.51, 0.79];
        for space in [
            RgbColorSpace::Srgb,
            RgbColorSpace::LinearSrgb,
            RgbColorSpace::DisplayP3,
            RgbColorSpace::LinearDisplayP3,
        ] {
            let round_trip = convert_rgb(color, space, space);
            for (actual, expected) in round_trip.into_iter().zip(color) {
                assert!((actual - expected).abs() < 0.000_01);
            }
        }
    }

    #[test]
    fn spaces_with_the_same_primaries_only_change_the_transfer_curve() {
        let color = [0.0, 0.5, 1.0];
        assert_eq!(
            to_linear_srgb(color, RgbColorSpace::Srgb),
            color.map(|value| RgbTransfer::Srgb.decode(value))
        );
        assert_eq!(
            convert_rgb(
                color,
                RgbColorSpace::DisplayP3,
                RgbColorSpace::LinearDisplayP3
            ),
            color.map(|value| RgbTransfer::Srgb.decode(value))
        );
    }
}
