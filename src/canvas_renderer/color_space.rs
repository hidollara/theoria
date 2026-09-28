use std::collections::HashMap;

use crate::{
    canvas::{Color, ColorValue, IccColorError},
    color::{
        ColorSpace, IccRenderingIntent, LinearDisplayP3, LinearSrgb, Transfer, connect_spaces,
    },
};

/// Renderer-owned conversions prepared before commands are recorded.
///
/// Keys identify colors within the borrowed, immutable Canvas for one render.
/// The Canvas stays alive and unmoved until this cache is discarded.
pub(crate) struct PreparedColors {
    working_space: ColorSpace,
    premultiplied: HashMap<usize, [f32; 4]>,
    interpolations: HashMap<ColorSpace, PreparedInterpolation>,
}

/// Conversion plan for one gradient's declared interpolation space.
pub(crate) struct PreparedInterpolation {
    to_interpolation: Transfer,
    to_working: Transfer,
    identity: bool,
    linear_builtin: bool,
}

impl PreparedInterpolation {
    pub(crate) fn is_linear_builtin(&self) -> bool {
        self.linear_builtin
    }

    fn convert(&self, rgb: [f32; 3], to_interpolation: bool) -> [f32; 3] {
        if self.identity {
            return rgb;
        }
        let transform = if to_interpolation {
            &self.to_interpolation
        } else {
            &self.to_working
        };
        let values = transform
            .evaluate(&rgb.map(f64::from))
            .expect("a prepared gradient space must evaluate finite RGB coordinates");
        [values[0] as f32, values[1] as f32, values[2] as f32]
    }
}

impl Default for PreparedColors {
    fn default() -> Self {
        Self::new(LinearSrgb)
    }
}

impl PreparedColors {
    pub(crate) fn new(working_space: impl Into<ColorSpace>) -> Self {
        Self {
            working_space: working_space.into(),
            premultiplied: HashMap::new(),
            interpolations: HashMap::new(),
        }
    }

    pub(crate) fn working_space(&self) -> &ColorSpace {
        &self.working_space
    }

    pub(crate) fn prepare_interpolation(
        &mut self,
        space: &ColorSpace,
    ) -> Result<(), IccColorError> {
        if self.interpolations.contains_key(space) {
            return Ok(());
        }
        if space.channels() != 3 {
            return Err(IccColorError::UnsupportedColorChannels(space.channels()));
        }
        let working = &self.working_space;
        let intent = IccRenderingIntent::RelativeColorimetric;
        let plan = PreparedInterpolation {
            to_interpolation: connect_spaces(working, space, intent, intent)?,
            to_working: connect_spaces(space, working, intent, intent)?,
            identity: space == working,
            linear_builtin: *space == ColorSpace::from(LinearSrgb)
                || *space == ColorSpace::from(LinearDisplayP3),
        };
        self.interpolations.insert(space.clone(), plan);
        Ok(())
    }

    pub(crate) fn interpolation(&self, space: &ColorSpace) -> &PreparedInterpolation {
        &self.interpolations[space]
    }

    pub(crate) fn prepare(&mut self, color: &Color) -> Result<(), IccColorError> {
        let key = color as *const Color as usize;
        if let std::collections::hash_map::Entry::Vacant(entry) = self.premultiplied.entry(key) {
            let [red, green, blue, alpha] = color.try_straight_rgba_in(
                &self.working_space,
                IccRenderingIntent::RelativeColorimetric,
            )?;
            entry.insert([red * alpha, green * alpha, blue * alpha, alpha]);
        }
        Ok(())
    }

    pub(crate) fn lower(&self, color: &Color) -> [f32; 4] {
        let key = color as *const Color as usize;
        self.premultiplied
            .get(&key)
            .copied()
            .unwrap_or_else(|| lower_straight_color_in(color, &self.working_space))
    }
}

impl Color {
    /// Resolves the declared source color into a renderer-selected RGB space.
    pub(crate) fn straight_rgba_in(
        &self,
        space: impl Into<ColorSpace>,
        intent: IccRenderingIntent,
    ) -> [f32; 4] {
        self.try_straight_rgba_in(space, intent)
            .expect("this caller requires an ICC conversion known to succeed")
    }

    pub(crate) fn try_straight_rgba_in(
        &self,
        space: impl Into<ColorSpace>,
        intent: IccRenderingIntent,
    ) -> Result<[f32; 4], IccColorError> {
        let coordinates: Vec<f64> = match &self.value {
            ColorValue::Dynamic(coordinates, _) => coordinates.to_vec(),
            _ => self.value.coordinates_as_f64().to_vec(),
        };
        let destination = space.into();
        if destination.channels() != 3 {
            return Err(IccColorError::UnsupportedColorChannels(
                destination.channels(),
            ));
        }
        let converted = if self.space == destination {
            coordinates
        } else {
            let transfer = connect_spaces(
                &self.space,
                &destination,
                intent,
                IccRenderingIntent::RelativeColorimetric,
            )?;
            transfer.evaluate(&coordinates)?
        };
        Ok([
            converted[0] as f32,
            converted[1] as f32,
            converted[2] as f32,
            self.value.alpha().as_f32(),
        ])
    }
}

impl ColorValue {
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

#[cfg(test)]
pub(crate) fn lower_straight_color(color: &Color) -> [f32; 4] {
    lower_straight_color_in(color, LinearSrgb)
}

pub(crate) fn lower_straight_color_in(color: &Color, space: impl Into<ColorSpace>) -> [f32; 4] {
    let [red, green, blue, alpha] =
        color.straight_rgba_in(space, IccRenderingIntent::RelativeColorimetric);
    [red * alpha, green * alpha, blue * alpha, alpha]
}

pub(super) fn interpolate_premultiplied_rgba(
    first: [f32; 4],
    second: [f32; 4],
    amount: f32,
    interpolation: &PreparedInterpolation,
) -> [f32; 4] {
    let first = convert_premultiplied(first, |rgb| interpolation.convert(rgb, true));
    let second = convert_premultiplied(second, |rgb| interpolation.convert(rgb, true));
    let mixed = std::array::from_fn(|index| first[index] + (second[index] - first[index]) * amount);
    convert_premultiplied(mixed, |rgb| interpolation.convert(rgb, false))
}

pub(super) fn mix_premultiplied_rgba(
    colors: [[f32; 4]; 3],
    weights: [f32; 3],
    interpolation: &PreparedInterpolation,
) -> [f32; 4] {
    let colors =
        colors.map(|color| convert_premultiplied(color, |rgb| interpolation.convert(rgb, true)));
    let mixed = std::array::from_fn(|component| {
        colors[0][component] * weights[0]
            + colors[1][component] * weights[1]
            + colors[2][component] * weights[2]
    });
    convert_premultiplied(mixed, |rgb| interpolation.convert(rgb, false))
}

pub(crate) fn convert_premultiplied_space(
    value: [f32; 4],
    from: &ColorSpace,
    to: &ColorSpace,
) -> Result<[f32; 4], Box<dyn std::error::Error>> {
    if from == to {
        return Ok(value);
    }
    if value[3] == 0.0 {
        return Ok([0.0; 4]);
    }
    let transfer = connect_spaces(
        from,
        to,
        IccRenderingIntent::RelativeColorimetric,
        IccRenderingIntent::RelativeColorimetric,
    )?;
    let alpha = value[3];
    let straight = [value[0] / alpha, value[1] / alpha, value[2] / alpha];
    let converted = transfer.evaluate(&straight.map(f64::from))?;
    Ok([
        converted[0] as f32 * alpha,
        converted[1] as f32 * alpha,
        converted[2] as f32 * alpha,
        alpha,
    ])
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
        let color = Color::new(Srgb, [255_u8, 0, 0], Opacity::<u8>::ONE);
        let mut prepared = PreparedColors::default();
        prepared.prepare(&color).unwrap();
        prepared.prepare(&color).unwrap();

        assert_eq!(prepared.premultiplied.len(), 1);
        assert_eq!(prepared.lower(&color), [1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn conversion_intent_is_chosen_when_the_color_is_used() {
        let color = Color::new(Srgb, [128_u8, 64, 32], Opacity::<u8>::ONE);
        assert!(
            color
                .try_straight_rgba_in(LinearSrgb, IccRenderingIntent::RelativeColorimetric)
                .is_ok()
        );
        assert!(
            color
                .try_straight_rgba_in(LinearSrgb, IccRenderingIntent::Perceptual)
                .is_err()
        );
    }

    #[test]
    fn icc_transfer_failure_is_reported_during_renderer_preparation() {
        let mut raw = moxcms::ColorProfile::new_srgb();
        raw.red_trc = None;
        raw.green_trc = None;
        raw.blue_trc = None;
        let profile = crate::color::IccProfile::from_bytes(&raw.encode().unwrap()).unwrap();
        let color = Color::from_icc(&profile, vec![0.25, 0.5, 0.75], Opacity::<f32>::ONE).unwrap();

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
        let color = Color::from_icc(&profile, vec![0.25, 0.5, 0.75], Opacity::<f32>::ONE).unwrap();
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
        let mut prepared = PreparedColors::default();
        prepared.prepare_interpolation(&Srgb.into()).unwrap();
        prepared
            .prepare_interpolation(&crate::color::LinearSrgb.into())
            .unwrap();
        let encoded =
            interpolate_premultiplied_rgba(black, white, 0.5, prepared.interpolation(&Srgb.into()));
        let linear = interpolate_premultiplied_rgba(
            black,
            white,
            0.5,
            prepared.interpolation(&crate::color::LinearSrgb.into()),
        );

        assert!(encoded[0] < linear[0]);
        assert!((encoded[0] - 0.214_041_14).abs() < 0.000_001);
        assert_eq!(linear, [0.5, 0.5, 0.5, 1.0]);
    }

    #[test]
    fn interpolation_accepts_a_noncanonical_space_handle() {
        let space = crate::test_support::distinct_color_space(Srgb);
        assert_ne!(space, ColorSpace::from(Srgb));
        let mut prepared = PreparedColors::default();
        prepared.prepare_interpolation(&space).unwrap();
        let actual = interpolate_premultiplied_rgba(
            [0.0, 0.0, 0.0, 1.0],
            [1.0, 1.0, 1.0, 1.0],
            0.5,
            prepared.interpolation(&space),
        );
        assert!((actual[0] - 0.214_041_14).abs() < 0.000_01);
    }

    #[test]
    fn preparation_uses_the_selected_working_space() {
        let color = Color::new(Srgb, [0.5_f32, 0.5, 0.5], Opacity::<f32>::ONE);
        let mut prepared = PreparedColors::new(Srgb);
        prepared.prepare(&color).unwrap();
        assert_eq!(prepared.lower(&color), [0.5, 0.5, 0.5, 1.0]);
    }

    #[test]
    fn interpolation_returns_coordinates_in_the_working_space() {
        let black = [0.0, 0.0, 0.0, 1.0];
        let white = [1.0, 1.0, 1.0, 1.0];
        let mut prepared = PreparedColors::new(LinearDisplayP3);
        prepared.prepare_interpolation(&Srgb.into()).unwrap();
        let actual =
            interpolate_premultiplied_rgba(black, white, 0.5, prepared.interpolation(&Srgb.into()));
        let expected = connect_spaces(
            Srgb,
            LinearDisplayP3,
            IccRenderingIntent::RelativeColorimetric,
            IccRenderingIntent::RelativeColorimetric,
        )
        .unwrap()
        .evaluate(&[0.5; 3])
        .unwrap();
        for (actual, expected) in actual[..3].iter().zip(expected) {
            assert!((actual - expected as f32).abs() < 0.000_01);
        }
        assert_eq!(actual[3], 1.0);
    }
}
