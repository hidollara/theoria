use crate::color::RgbColorSpace;

pub(crate) fn lower_straight_srgba([red, green, blue, alpha]: [f32; 4]) -> [f32; 4] {
    let [red, green, blue] = to_linear_srgb([red, green, blue], RgbColorSpace::Srgb);
    [red * alpha, green * alpha, blue * alpha, alpha]
}

pub(crate) fn encode_linear_srgb_component(value: f32) -> f32 {
    encode_srgb(value)
}

pub(super) fn interpolate_premultiplied_linear_srgba(
    first: [f32; 4],
    second: [f32; 4],
    amount: f32,
    space: RgbColorSpace,
) -> [f32; 4] {
    let first = from_working(first, space);
    let second = from_working(second, space);
    let mixed = std::array::from_fn(|index| first[index] + (second[index] - first[index]) * amount);
    to_working(mixed, space)
}

pub(super) fn mix_premultiplied_linear_srgba(
    colors: [[f32; 4]; 3],
    weights: [f32; 3],
    space: RgbColorSpace,
) -> [f32; 4] {
    let colors = colors.map(|color| from_working(color, space));
    let mixed = std::array::from_fn(|component| {
        colors[0][component] * weights[0]
            + colors[1][component] * weights[1]
            + colors[2][component] * weights[2]
    });
    to_working(mixed, space)
}

fn from_working(value: [f32; 4], space: RgbColorSpace) -> [f32; 4] {
    convert_premultiplied(value, |rgb| from_linear_srgb(rgb, space))
}

fn to_working(value: [f32; 4], space: RgbColorSpace) -> [f32; 4] {
    convert_premultiplied(value, |rgb| to_linear_srgb(rgb, space))
}

fn convert_premultiplied(
    [red, green, blue, alpha]: [f32; 4],
    convert: impl FnOnce([f32; 3]) -> [f32; 3],
) -> [f32; 4] {
    if alpha == 0.0 {
        return [0.0; 4];
    }
    let converted = convert([red / alpha, green / alpha, blue / alpha]);
    [
        converted[0] * alpha,
        converted[1] * alpha,
        converted[2] * alpha,
        alpha,
    ]
}

pub(crate) fn from_linear_srgb(rgb: [f32; 3], space: RgbColorSpace) -> [f32; 3] {
    match space {
        RgbColorSpace::Srgb => rgb.map(encode_srgb),
        RgbColorSpace::LinearSrgb => rgb,
        RgbColorSpace::DisplayP3 => linear_srgb_to_linear_p3(rgb).map(encode_srgb),
        RgbColorSpace::LinearDisplayP3 => linear_srgb_to_linear_p3(rgb),
    }
}

pub(crate) fn to_linear_srgb(rgb: [f32; 3], space: RgbColorSpace) -> [f32; 3] {
    match space {
        RgbColorSpace::Srgb => rgb.map(decode_srgb),
        RgbColorSpace::LinearSrgb => rgb,
        RgbColorSpace::DisplayP3 => linear_p3_to_linear_srgb(rgb.map(decode_srgb)),
        RgbColorSpace::LinearDisplayP3 => linear_p3_to_linear_srgb(rgb),
    }
}

fn linear_srgb_to_linear_p3(rgb: [f32; 3]) -> [f32; 3] {
    multiply(XYZ_D65_TO_LINEAR_P3, multiply(LINEAR_SRGB_TO_XYZ_D65, rgb))
}

fn linear_p3_to_linear_srgb(rgb: [f32; 3]) -> [f32; 3] {
    multiply(XYZ_D65_TO_LINEAR_SRGB, multiply(LINEAR_P3_TO_XYZ_D65, rgb))
}

fn multiply(matrix: [[f32; 3]; 3], vector: [f32; 3]) -> [f32; 3] {
    matrix.map(|row| row[0] * vector[0] + row[1] * vector[1] + row[2] * vector[2])
}

fn decode_srgb(value: f32) -> f32 {
    let magnitude = value.abs();
    if magnitude <= 0.04045 {
        value / 12.92
    } else {
        value.signum() * ((magnitude + 0.055) / 1.055).powf(2.4)
    }
}

fn encode_srgb(value: f32) -> f32 {
    let magnitude = value.abs();
    if magnitude <= 0.003_130_8 {
        12.92 * value
    } else {
        value.signum() * (1.055 * magnitude.powf(1.0 / 2.4) - 0.055)
    }
}

// Exact rational matrices from CSS Color 4's color-conversion sample code.
const LINEAR_SRGB_TO_XYZ_D65: [[f32; 3]; 3] = [
    [506752.0 / 1228815.0, 87881.0 / 245763.0, 12673.0 / 70218.0],
    [87098.0 / 409605.0, 175762.0 / 245763.0, 12673.0 / 175545.0],
    [7918.0 / 409605.0, 87881.0 / 737289.0, 1001167.0 / 1053270.0],
];
const XYZ_D65_TO_LINEAR_SRGB: [[f32; 3]; 3] = [
    [12831.0 / 3959.0, -329.0 / 214.0, -1974.0 / 3959.0],
    [
        -851781.0 / 878810.0,
        1648619.0 / 878810.0,
        36519.0 / 878810.0,
    ],
    [705.0 / 12673.0, -2585.0 / 12673.0, 705.0 / 667.0],
];
const LINEAR_P3_TO_XYZ_D65: [[f32; 3]; 3] = [
    [
        608311.0 / 1250200.0,
        189793.0 / 714400.0,
        198249.0 / 1000160.0,
    ],
    [
        35783.0 / 156275.0,
        247089.0 / 357200.0,
        198249.0 / 2500400.0,
    ],
    [0.0, 32229.0 / 714400.0, 5220557.0 / 5000800.0],
];
const XYZ_D65_TO_LINEAR_P3: [[f32; 3]; 3] = [
    [
        446124.0 / 178915.0,
        -333277.0 / 357830.0,
        -72051.0 / 178915.0,
    ],
    [-14852.0 / 17905.0, 63121.0 / 35810.0, 423.0 / 17905.0],
    [11844.0 / 330415.0, -50337.0 / 660830.0, 316169.0 / 330415.0],
];

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
            let round_trip = from_linear_srgb(to_linear_srgb(color, space), space);
            for (actual, expected) in round_trip.into_iter().zip(color) {
                assert!((actual - expected).abs() < 0.000_01);
            }
        }
    }

    #[test]
    fn interpolation_is_performed_in_the_selected_space() {
        let black = [0.0, 0.0, 0.0, 1.0];
        let white = [1.0, 1.0, 1.0, 1.0];
        let encoded =
            interpolate_premultiplied_linear_srgba(black, white, 0.5, RgbColorSpace::Srgb);
        let linear =
            interpolate_premultiplied_linear_srgba(black, white, 0.5, RgbColorSpace::LinearSrgb);

        assert!(encoded[0] < linear[0]);
        assert!((encoded[0] - 0.214_041_14).abs() < 0.000_001);
        assert_eq!(linear, [0.5, 0.5, 0.5, 1.0]);
    }
}
