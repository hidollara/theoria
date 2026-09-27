use std::collections::HashMap;

use crate::{
    canvas::{CanvasColor, CanvasIccColorError, CanvasProfile, ColorValue},
    color::{
        IccRenderingIntent, LabD50, RgbColorSpace, XyzD50Profile, XyzD65, connect_profiles,
        convert_rgb,
    },
};

/// Renderer-owned conversions prepared before commands are recorded.
///
/// Keys identify colors within the borrowed, immutable Canvas for one render.
/// The Canvas stays alive and unmoved until this cache is discarded.
pub(crate) struct PreparedColors {
    working_space: RgbColorSpace,
    premultiplied: HashMap<usize, [f32; 4]>,
}

impl Default for PreparedColors {
    fn default() -> Self {
        Self::new(RgbColorSpace::LinearSrgb)
    }
}

impl PreparedColors {
    pub(crate) fn new(working_space: RgbColorSpace) -> Self {
        Self {
            working_space,
            premultiplied: HashMap::new(),
        }
    }

    pub(crate) fn working_space(&self) -> RgbColorSpace {
        self.working_space
    }

    pub(crate) fn prepare(&mut self, color: &CanvasColor) -> Result<(), CanvasIccColorError> {
        let key = color as *const CanvasColor as usize;
        if let std::collections::hash_map::Entry::Vacant(entry) = self.premultiplied.entry(key) {
            let [red, green, blue, alpha] = color.try_straight_rgba_in(self.working_space)?;
            entry.insert([red * alpha, green * alpha, blue * alpha, alpha]);
        }
        Ok(())
    }

    pub(crate) fn lower(&self, color: &CanvasColor) -> [f32; 4] {
        let key = color as *const CanvasColor as usize;
        self.premultiplied
            .get(&key)
            .copied()
            .unwrap_or_else(|| lower_straight_color_in(color, self.working_space))
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
                crate::color::convert_rgb(self.value.coordinates_as_f32(), *authored_space, space)
            }
            CanvasProfile::XyzD50 => space
                .xyz_d50_to_coordinates(self.value.coordinates_as_f64())
                .map(|value| value as f32),
            CanvasProfile::XyzD65 => {
                let xyz = XyzD65.coordinates_to_xyz_d50(&self.value.coordinates_as_f64());
                space.xyz_d50_to_coordinates(xyz).map(|value| value as f32)
            }
            CanvasProfile::LabD50 => {
                let xyz = LabD50.coordinates_to_xyz_d50(&self.value.coordinates_as_f64());
                space.xyz_d50_to_coordinates(xyz).map(|value| value as f32)
            }
            CanvasProfile::Icc { profile, intent } => {
                let ColorValue::Dynamic(coordinates, _) = &self.value else {
                    unreachable!("ICC profile always stores dynamic device coordinates")
                };
                convert_icc_color(profile, *intent, coordinates, space)?
            }
        };
        Ok([rgb[0], rgb[1], rgb[2], self.value.alpha().as_f32()])
    }
}

impl ColorValue {
    fn coordinates_as_f32(&self) -> [f32; 3] {
        match self {
            Self::F32(coordinates, _) => *coordinates,
            Self::F64(coordinates, _) => coordinates.map(|value| value as f32),
            Self::U8(coordinates, _) => {
                coordinates.map(|value| f32::from(value) / f32::from(u8::MAX))
            }
            Self::U16(coordinates, _) => {
                coordinates.map(|value| f32::from(value) / f32::from(u16::MAX))
            }
            Self::Dynamic(..) => unreachable!("ICC coordinates are evaluated by their profile"),
        }
    }

    fn coordinates_as_f64(&self) -> [f64; 3] {
        match self {
            Self::F32(coordinates, _) => coordinates.map(f64::from),
            Self::F64(coordinates, _) => *coordinates,
            Self::U8(coordinates, _) => {
                coordinates.map(|value| f64::from(value) / f64::from(u8::MAX))
            }
            Self::U16(coordinates, _) => {
                coordinates.map(|value| f64::from(value) / f64::from(u16::MAX))
            }
            Self::Dynamic(..) => unreachable!("ICC coordinates are evaluated by their profile"),
        }
    }
}

fn convert_icc_color(
    profile: &crate::color::IccProfile,
    intent: crate::color::IccRenderingIntent,
    coordinates: &[f64],
    space: RgbColorSpace,
) -> Result<[f32; 3], CanvasIccColorError> {
    let transfer = connect_profiles(
        profile,
        space,
        intent,
        IccRenderingIntent::RelativeColorimetric,
    )?;
    let converted = transfer.transfer().evaluate(coordinates)?;
    Ok([
        converted[0] as f32,
        converted[1] as f32,
        converted[2] as f32,
    ])
}

#[cfg(test)]
pub(crate) fn lower_straight_color(color: &CanvasColor) -> [f32; 4] {
    lower_straight_color_in(color, RgbColorSpace::LinearSrgb)
}

pub(crate) fn lower_straight_color_in(color: &CanvasColor, space: RgbColorSpace) -> [f32; 4] {
    let [red, green, blue, alpha] = color.straight_rgba_in(space);
    [red * alpha, green * alpha, blue * alpha, alpha]
}

pub(crate) use crate::color::encode_srgb as encode_linear_srgb_component;

pub(super) fn interpolate_premultiplied_rgba(
    first: [f32; 4],
    second: [f32; 4],
    amount: f32,
    interpolation_space: RgbColorSpace,
    working_space: RgbColorSpace,
) -> [f32; 4] {
    let first = convert_premultiplied_space(first, working_space, interpolation_space);
    let second = convert_premultiplied_space(second, working_space, interpolation_space);
    let mixed = std::array::from_fn(|index| first[index] + (second[index] - first[index]) * amount);
    convert_premultiplied_space(mixed, interpolation_space, working_space)
}

pub(super) fn mix_premultiplied_rgba(
    colors: [[f32; 4]; 3],
    weights: [f32; 3],
    interpolation_space: RgbColorSpace,
    working_space: RgbColorSpace,
) -> [f32; 4] {
    let colors =
        colors.map(|color| convert_premultiplied_space(color, working_space, interpolation_space));
    let mixed = std::array::from_fn(|component| {
        colors[0][component] * weights[0]
            + colors[1][component] * weights[1]
            + colors[2][component] * weights[2]
    });
    convert_premultiplied_space(mixed, interpolation_space, working_space)
}

pub(crate) fn convert_premultiplied_space(
    value: [f32; 4],
    from: RgbColorSpace,
    to: RgbColorSpace,
) -> [f32; 4] {
    if from == to {
        return value;
    }
    convert_premultiplied(value, |rgb| convert_rgb(rgb, from, to))
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
    use crate::color::{Opacity, Srgb};

    #[test]
    fn a_solid_color_is_lowered_once_per_render() {
        let color = CanvasColor::from(crate::color::Color::new(
            Srgb,
            [255_u8, 0, 0],
            Opacity::<u8>::ONE,
        ));
        let mut prepared = PreparedColors::default();
        prepared.prepare(&color).unwrap();
        prepared.prepare(&color).unwrap();

        assert_eq!(prepared.premultiplied.len(), 1);
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
            Opacity::<f32>::ONE,
            crate::color::IccRenderingIntent::RelativeColorimetric,
        )
        .unwrap();

        let mut prepared = PreparedColors::default();
        assert!(prepared.prepare(&color).is_err());
    }

    #[test]
    fn preparing_an_icc_color_does_not_rewrite_authored_data() {
        let profile = crate::color::IccProfile::from_bytes(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/crates/theoria-color/src/color/fixtures/sRGB-v4.icc"
        )))
        .unwrap();
        let color = CanvasColor::from_icc(
            &profile,
            vec![0.25, 0.5, 0.75],
            Opacity::<f32>::ONE,
            crate::color::IccRenderingIntent::RelativeColorimetric,
        )
        .unwrap();
        let original = color.clone();
        let mut prepared = PreparedColors::default();
        prepared.prepare(&color).unwrap();
        assert_eq!(color, original);
        assert!(prepared.lower(&color)[0] > 0.0);
        assert!(matches!(
            &color.value,
            ColorValue::Dynamic(coordinates, _) if coordinates.as_ref() == [0.25, 0.5, 0.75]
        ));
    }

    #[test]
    fn interpolation_is_performed_in_the_selected_space() {
        let black = [0.0, 0.0, 0.0, 1.0];
        let white = [1.0, 1.0, 1.0, 1.0];
        let encoded = interpolate_premultiplied_rgba(
            black,
            white,
            0.5,
            RgbColorSpace::Srgb,
            RgbColorSpace::LinearSrgb,
        );
        let linear = interpolate_premultiplied_rgba(
            black,
            white,
            0.5,
            RgbColorSpace::LinearSrgb,
            RgbColorSpace::LinearSrgb,
        );

        assert!(encoded[0] < linear[0]);
        assert!((encoded[0] - 0.214_041_14).abs() < 0.000_001);
        assert_eq!(linear, [0.5, 0.5, 0.5, 1.0]);
    }

    #[test]
    fn preparation_uses_the_selected_working_space() {
        let color = CanvasColor::from(crate::color::Color::new(
            Srgb,
            [0.5_f32, 0.5, 0.5],
            Opacity::<f32>::ONE,
        ));
        let mut prepared = PreparedColors::new(RgbColorSpace::Srgb);
        prepared.prepare(&color).unwrap();
        assert_eq!(prepared.lower(&color), [0.5, 0.5, 0.5, 1.0]);
    }

    #[test]
    fn interpolation_returns_coordinates_in_the_working_space() {
        let black = [0.0, 0.0, 0.0, 1.0];
        let white = [1.0, 1.0, 1.0, 1.0];
        let actual = interpolate_premultiplied_rgba(
            black,
            white,
            0.5,
            RgbColorSpace::Srgb,
            RgbColorSpace::LinearDisplayP3,
        );
        let expected = convert_rgb(
            [0.5; 3],
            RgbColorSpace::Srgb,
            RgbColorSpace::LinearDisplayP3,
        );
        for (actual, expected) in actual[..3].iter().zip(expected) {
            assert!((actual - expected).abs() < 0.000_01);
        }
        assert_eq!(actual[3], 1.0);
    }
}
