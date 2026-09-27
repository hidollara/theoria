use super::{RgbColorSpace, transform};

/// Converts straight RGB coordinates between the predefined Canvas spaces.
///
/// This operates on logical coordinates only. Alpha association and renderer
/// working precision are deliberately outside this boundary.
pub fn convert_rgb(rgb: [f32; 3], from: RgbColorSpace, to: RgbColorSpace) -> [f32; 3] {
    if from == to {
        rgb
    } else {
        let decode = |value| {
            if from.is_linear() {
                value
            } else {
                transform::decode_srgb(value)
            }
        };
        let encode = |value| {
            if to.is_linear() {
                value
            } else {
                transform::encode_srgb(value)
            }
        };
        if from.matrices().0 == to.matrices().0 {
            rgb.map(|value| encode(decode(value)))
        } else {
            let linear = rgb.map(decode);
            let xyz = multiply(from.matrices().0, linear);
            multiply(to.matrices().1, xyz).map(encode)
        }
    }
}

fn multiply(matrix: [[f64; 3]; 3], vector: [f32; 3]) -> [f32; 3] {
    matrix.map(|row| {
        row[0] as f32 * vector[0] + row[1] as f32 * vector[1] + row[2] as f32 * vector[2]
    })
}

pub fn encode_srgb(value: f32) -> f32 {
    transform::encode_srgb(value)
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
            convert_rgb(color, RgbColorSpace::Srgb, RgbColorSpace::LinearSrgb),
            color.map(transform::decode_srgb)
        );
        assert_eq!(
            convert_rgb(
                color,
                RgbColorSpace::DisplayP3,
                RgbColorSpace::LinearDisplayP3
            ),
            color.map(transform::decode_srgb)
        );
    }
}
