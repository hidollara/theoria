use crate::{
    canvas::{CanvasColor, CanvasIccColorError, CanvasProfile, Coordinates},
    color::{
        ColorTransform, LabD50, LinearSrgb, RgbColorSpace, XyzD50Profile, XyzD65, from_linear_srgb,
        to_linear_srgb,
    },
};

impl CanvasColor {
    /// Performs fallible ICC lowering once before a renderer records commands.
    /// Clones of a Canvas color share the prepared coordinates, while the
    /// original profile and device coordinates remain available on the Canvas.
    pub(crate) fn prepare_linear_srgb(&self) -> Result<(), CanvasIccColorError> {
        self.try_straight_rgba_in(LinearSrgb.into()).map(|_| ())
    }

    /// Resolves the declared source color into a renderer-selected RGB space.
    pub(crate) fn straight_rgba_in(&self, space: RgbColorSpace) -> [f32; 4] {
        self.try_straight_rgba_in(space)
            .expect("ICC colors must be prepared before infallible rendering")
    }

    pub(crate) fn try_straight_rgba_in(
        &self,
        space: RgbColorSpace,
    ) -> Result<[f32; 4], CanvasIccColorError> {
        let rgb = match &self.profile {
            CanvasProfile::Rgb(authored_space) => {
                crate::color::convert_rgb(self.coordinates.as_f32(), *authored_space, space)
            }
            CanvasProfile::XyzD50 => space
                .xyz_d50_to_coordinates(self.coordinates.as_f64())
                .map(|value| value as f32),
            CanvasProfile::XyzD65 => {
                let xyz = XyzD65.coordinates_to_xyz_d50(&self.coordinates.as_f64());
                space.xyz_d50_to_coordinates(xyz).map(|value| value as f32)
            }
            CanvasProfile::LabD50 => {
                let xyz = LabD50.coordinates_to_xyz_d50(&self.coordinates.as_f64());
                space.xyz_d50_to_coordinates(xyz).map(|value| value as f32)
            }
            CanvasProfile::Icc {
                profile,
                intent,
                prepared_linear_srgb,
            } => {
                let Coordinates::Dynamic(coordinates) = &self.coordinates else {
                    unreachable!("ICC profile always stores dynamic device coordinates")
                };
                if space == RgbColorSpace::LinearSrgb {
                    if let Some(cached) = prepared_linear_srgb.get() {
                        *cached
                    } else {
                        let converted = convert_icc_color(profile, *intent, coordinates, space)?;
                        let _ = prepared_linear_srgb.set(converted);
                        converted
                    }
                } else {
                    convert_icc_color(profile, *intent, coordinates, space)?
                }
            }
        };
        Ok([rgb[0], rgb[1], rgb[2], self.opacity.as_f32()])
    }
}

fn convert_icc_color(
    profile: &crate::color::IccProfile,
    intent: crate::color::IccRenderingIntent,
    coordinates: &[f64],
    space: RgbColorSpace,
) -> Result<[f32; 3], CanvasIccColorError> {
    let source = profile.to_pcs(intent)?;
    let target = ColorTransform::pcs_to_rgb(profile.pcs(), space)?;
    let transfer = source.then(&target)?;
    let converted = transfer.transfer().evaluate(coordinates)?;
    Ok([
        converted[0] as f32,
        converted[1] as f32,
        converted[2] as f32,
    ])
}

pub(crate) fn lower_straight_color(color: &CanvasColor) -> [f32; 4] {
    let [red, green, blue, alpha] = color.straight_rgba_in(LinearSrgb.into());
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
