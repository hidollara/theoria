use crate::color::{RgbColorSpace, from_linear_srgb, to_linear_srgb};

pub(crate) fn lower_straight_rgba(
    [red, green, blue, alpha]: [f32; 4],
    space: RgbColorSpace,
) -> [f32; 4] {
    let [red, green, blue] = to_linear_srgb([red, green, blue], space);
    [red * alpha, green * alpha, blue * alpha, alpha]
}

pub(crate) use crate::color::encode_srgb as encode_linear_srgb_component;

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

#[cfg(test)]
mod tests {
    use super::*;

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
