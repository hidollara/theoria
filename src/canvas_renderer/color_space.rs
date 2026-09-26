use std::collections::HashMap;

use crate::{
    canvas::{CanvasColor, CanvasIccColorError, CanvasProfile, Coordinates},
    color::{
        ColorTransform, LabD50, LinearSrgb, RgbColorSpace, XyzD50Profile, XyzD65, from_linear_srgb,
        to_linear_srgb,
    },
};

/// Renderer-owned conversions prepared before commands are recorded.
///
/// Keys identify colors within the borrowed, immutable Canvas for one render.
/// The Canvas stays alive and unmoved until this cache is discarded.
#[derive(Default)]
pub(crate) struct PreparedColors {
    premultiplied_linear_srgb: HashMap<usize, [f32; 4]>,
}

impl PreparedColors {
    pub(crate) fn prepare(&mut self, color: &CanvasColor) -> Result<(), CanvasIccColorError> {
        let key = color as *const CanvasColor as usize;
        if let std::collections::hash_map::Entry::Vacant(entry) =
            self.premultiplied_linear_srgb.entry(key)
        {
            let [red, green, blue, alpha] = color.try_straight_rgba_in(LinearSrgb.into())?;
            entry.insert([red * alpha, green * alpha, blue * alpha, alpha]);
        }
        Ok(())
    }

    pub(crate) fn lower(&self, color: &CanvasColor) -> [f32; 4] {
        let key = color as *const CanvasColor as usize;
        self.premultiplied_linear_srgb
            .get(&key)
            .copied()
            .unwrap_or_else(|| lower_straight_color(color))
    }
}

impl CanvasColor {
    /// Resolves the declared source color into a renderer-selected RGB space.
    pub(crate) fn straight_rgba_in(&self, space: RgbColorSpace) -> [f32; 4] {
        self.try_straight_rgba_in(space)
            .expect("this caller requires an ICC conversion known to succeed")
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
            CanvasProfile::Icc { profile, intent } => {
                let Coordinates::Dynamic(coordinates) = &self.coordinates else {
                    unreachable!("ICC profile always stores dynamic device coordinates")
                };
                convert_icc_color(profile, *intent, coordinates, space)?
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
    fn a_solid_color_is_lowered_once_per_render() {
        let color = CanvasColor::from(crate::color::Color::rgb8(255, 0, 0));
        let mut prepared = PreparedColors::default();
        prepared.prepare(&color).unwrap();
        prepared.prepare(&color).unwrap();

        assert_eq!(prepared.premultiplied_linear_srgb.len(), 1);
        assert_eq!(prepared.lower(&color), [1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn icc_transfer_failure_is_reported_during_renderer_preparation() {
        let mut raw = moxcms::ColorProfile::new_srgb();
        raw.red_trc = None;
        raw.green_trc = None;
        raw.blue_trc = None;
        let profile = crate::color::IccProfile::from_bytes(&raw.encode().unwrap()).unwrap();
        let color = CanvasColor::from_icc(
            &profile,
            vec![0.25, 0.5, 0.75],
            crate::color::UnitF32::ONE,
            crate::color::IccRenderingIntent::RelativeColorimetric,
        )
        .unwrap();

        let mut prepared = PreparedColors::default();
        assert!(prepared.prepare(&color).is_err());
    }

    #[test]
    fn preparing_an_icc_color_does_not_rewrite_authored_data() {
        let profile =
            crate::color::IccProfile::from_bytes(include_bytes!("../color/fixtures/sRGB-v4.icc"))
                .unwrap();
        let color = CanvasColor::from_icc(
            &profile,
            vec![0.25, 0.5, 0.75],
            crate::color::UnitF32::ONE,
            crate::color::IccRenderingIntent::RelativeColorimetric,
        )
        .unwrap();
        let original = color.clone();
        let mut prepared = PreparedColors::default();
        prepared.prepare(&color).unwrap();
        assert_eq!(color, original);
        assert!(prepared.lower(&color)[0] > 0.0);
        assert_eq!(
            color.coordinates,
            Coordinates::Dynamic(vec![0.25, 0.5, 0.75].into())
        );
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
