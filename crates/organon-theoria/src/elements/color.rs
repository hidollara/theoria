//! Colors, color sources, and composition spaces.
use crate::attribute::{ParseContext, ParseError, ResolveContext, braced_reference};
use crate::{Source, Type, implement};
use theoria_canvas::{Color, ColorField, Opacity, color::Srgb};

#[implement]
impl Color {
    type Value = Color;

    #[parse]
    const fn parse<'a>(
        text: &'a str,
        cx: &mut ParseContext<'_>,
    ) -> Result<
        impl FnOnce(&mut ResolveContext<'_>) -> Result<Source<Self::Value>, String> + use<'a>,
        ParseError,
    > {
        color::<Self>(text, cx, |color| color)
    }

    #[output(name = "field", interface = "ColorSource")]
    fn color_field(value: Color) -> ColorField {
        value.into()
    }
}

#[implement]
impl ColorField {
    type Value = ColorField;

    #[parse]
    const fn parse<'a>(
        text: &'a str,
        cx: &mut ParseContext<'_>,
    ) -> Result<
        impl FnOnce(&mut ResolveContext<'_>) -> Result<Source<Self::Value>, String> + use<'a>,
        ParseError,
    > {
        color::<Self>(text, cx, ColorField::from)
    }

    #[output(name = "field", interface = "ColorSource")]
    fn own_field(value: ColorField) -> ColorField {
        value
    }
}

#[implement]
impl ColorSource {
    #[parse]
    const fn parse<'a>(
        text: &'a str,
        cx: &mut ParseContext<'_>,
    ) -> Result<
        impl FnOnce(&mut ResolveContext<'_>) -> Result<Source<Self>, String> + use<'a>,
        ParseError,
    > {
        enum Parsed<R, F> {
            Reference(R),
            Color(F),
        }

        let parsed =
            if !text.is_empty() && text.as_bytes()[0] != b'#' && braced_reference(text).is_none() {
                match cx.reference::<Self>(text) {
                    Ok(reference) => Parsed::Reference(reference),
                    Err(error) => return Err(error),
                }
            } else {
                match color::<Self>(text, cx, |color| {
                    crate::Provide::<Self>::provide(&ColorField::from(color))
                }) {
                    Ok(build) => Parsed::Color(build),
                    Err(error) => return Err(error),
                }
            };
        Ok(move |resolve: &mut ResolveContext<'_>| match parsed {
            Parsed::Reference(reference) => reference.resolve(resolve),
            Parsed::Color(build) => build(resolve),
        })
    }
}

// Parsing stores components and a constructor. Color creation remains ordinary
// Rust code, even though syntax and references can be resolved in a const context.
const fn color<'a, M: Type>(
    text: &'a str,
    cx: &mut ParseContext<'_>,
    construct: fn(Color) -> M::Value,
) -> Result<
    impl FnOnce(&mut ResolveContext<'_>) -> Result<Source<M::Value>, String> + Copy + use<'a, M>,
    ParseError,
> {
    #[derive(Clone, Copy)]
    enum Parsed<R> {
        Reference(R),
        Rgba([u8; 4]),
    }

    let parsed = match braced_reference(text) {
        Some(name) => match cx.reference::<M>(name) {
            Ok(reference) => Parsed::Reference(reference),
            Err(error) => return Err(error),
        },
        None => match components(text) {
            Ok(rgba) => Parsed::Rgba(rgba),
            Err(error) => return Err(error),
        },
    };
    Ok(move |resolve: &mut ResolveContext<'_>| match parsed {
        Parsed::Reference(reference) => reference.resolve(resolve),
        Parsed::Rgba(rgba) => Ok(Source::from(construct(Color::new(
            Srgb,
            [rgba[0], rgba[1], rgba[2]],
            Opacity::<u8>::from(rgba[3]),
        )))),
    })
}

const fn components(text: &str) -> Result<[u8; 4], ParseError> {
    let bytes = text.as_bytes();
    if (bytes.len() != 7 && bytes.len() != 9) || bytes[0] != b'#' {
        return Err(ParseError::new("color must be #RRGGBB or #RRGGBBAA"));
    }
    let mut rgba = [255; 4];
    let mut i = 1;
    while i < bytes.len() {
        let high = match hex(bytes[i]) {
            Some(value) => value,
            None => return Err(ParseError::new("invalid color digit")),
        };
        let low = match hex(bytes[i + 1]) {
            Some(value) => value,
            None => return Err(ParseError::new("invalid color digit")),
        };
        rgba[(i - 1) / 2] = high * 16 + low;
        i += 2;
    }
    Ok(rgba)
}

const fn hex(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
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
    const fn parse<'a>(
        text: &'a str,
        cx: &mut ParseContext<'_>,
    ) -> Result<
        impl FnOnce(&mut ResolveContext<'_>) -> Result<Source<Self::Value>, String> + use<'a>,
        ParseError,
    > {
        enum Parsed<R> {
            Reference(R),
            Space(Space),
        }

        let parsed = match braced_reference(text) {
            Some(name) => match cx.reference::<Self>(name) {
                Ok(reference) => Parsed::Reference(reference),
                Err(error) => return Err(error),
            },
            None => Parsed::Space(match text.as_bytes() {
                b"Srgb" => Space::Srgb,
                b"LinearSrgb" => Space::LinearSrgb,
                b"DisplayP3" => Space::DisplayP3,
                b"LinearDisplayP3" => Space::LinearDisplayP3,
                _ => return Err(ParseError::new("unknown composition space")),
            }),
        };
        Ok(move |resolve: &mut ResolveContext<'_>| match parsed {
            Parsed::Reference(reference) => reference.resolve(resolve),
            Parsed::Space(space) => Ok(Source::from(space)),
        })
    }
}
