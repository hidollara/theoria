//! Colors, color sources, and composition spaces.

use crate::implement;
use theoria_canvas::{Color, ColorField, Opacity, color::Srgb};

#[implement]
impl Color {
    type Value = Color;

    #[parse]
    fn parse(
        text: &str,
        cx: &mut crate::attribute::ParseContext<'_>,
    ) -> Result<crate::Source<Self::Value>, String> {
        crate::attribute::reference_or_literal::<Self>(text, cx, Self::parse_literal)
    }

    #[output(name = "field", interface = "ColorSource")]
    fn color_field(value: Color) -> ColorField {
        value.into()
    }

    fn parse_literal(text: &str) -> Result<Color, &'static str> {
        let hex = text
            .strip_prefix('#')
            .filter(|s| matches!(s.len(), 6 | 8) && s.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or("color must expand to #RRGGBB or #RRGGBBAA")?;
        let mut rgba = [255u8; 4];
        for (i, v) in rgba.iter_mut().take(hex.len() / 2).enumerate() {
            *v = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).map_err(|_| "invalid color")?;
        }
        Ok(Color::new(
            Srgb,
            [rgba[0], rgba[1], rgba[2]],
            Opacity::<u8>::from(rgba[3]),
        ))
    }
}

#[implement]
impl ColorField {
    type Value = ColorField;

    #[parse]
    fn parse(
        text: &str,
        cx: &mut crate::attribute::ParseContext<'_>,
    ) -> Result<crate::Source<Self::Value>, String> {
        crate::attribute::reference_or_literal::<Self>(text, cx, |text| {
            crate::types::Color::parse_literal(text).map(ColorField::from)
        })
    }

    #[output(name = "field", interface = "ColorSource")]
    fn own_field(value: ColorField) -> ColorField {
        value
    }
}

#[implement]
impl ColorSource {
    #[parse]
    fn parse(
        text: &str,
        cx: &mut crate::attribute::ParseContext<'_>,
    ) -> Result<crate::Source<Self>, String> {
        if !text.starts_with('#') && !text.contains('{') && !text.contains('}') {
            return cx.reference::<Self>(text);
        }
        crate::attribute::reference_or_literal::<Self>(text, cx, |text| {
            crate::types::Color::parse_literal(text)
                .map(ColorField::from)
                .map(|field| crate::Provide::<Self>::provide(&field))
        })
    }
}

#[derive(Clone, Debug)]
pub enum Space {
    Srgb,
    LinearSrgb,
    DisplayP3,
    LinearDisplayP3,
}

#[implement]
impl Space {
    type Value = Space;

    #[parse]
    fn parse(
        text: &str,
        cx: &mut crate::attribute::ParseContext<'_>,
    ) -> Result<crate::Source<Self::Value>, String> {
        crate::attribute::reference_or_literal::<Self>(text, cx, |text| match text {
            "Srgb" => Ok(Space::Srgb),
            "LinearSrgb" => Ok(Space::LinearSrgb),
            "DisplayP3" => Ok(Space::DisplayP3),
            "LinearDisplayP3" => Ok(Space::LinearDisplayP3),
            _ => Err("unknown composition space"),
        })
    }
}
