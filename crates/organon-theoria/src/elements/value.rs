//! Scalar and string attribute interpretation.

use crate::implement;

mod decimal;

#[implement]
impl DrawingError {
    type Value = &'static str;
}

#[implement]
impl number {
    type Value = f32;

    #[parse]
    const fn parse<'a>(
        text: &'a str,
        cx: &mut crate::attribute::ParseContext<'_>,
    ) -> Result<
        impl FnOnce(
            &mut crate::attribute::ResolveContext<'_>,
        ) -> Result<crate::Source<Self::Value>, String>
        + use<'a>,
        crate::attribute::ParseError,
    > {
        enum Parsed<R> {
            Decimal(f32),
            Reference(R),
        }

        let parsed = if let Some(name) = crate::attribute::braced_reference(text) {
            match cx.reference::<Self>(name) {
                Ok(reference) => Parsed::Reference(reference),
                Err(error) => return Err(error),
            }
        } else {
            match decimal::parse(text) {
                Ok(value) => Parsed::Decimal(value),
                Err(error) => return Err(crate::attribute::ParseError::new(error)),
            }
        };
        Ok(
            move |cx: &mut crate::attribute::ResolveContext<'_>| match parsed {
                Parsed::Reference(reference) => reference.resolve(cx),
                Parsed::Decimal(value) => Ok(crate::Source::from(value)),
            },
        )
    }

    #[output(interface = "Display", name = "text")]
    fn display(value: Self::Value) -> String {
        value.to_string()
    }
}

#[implement]
impl String {
    type Value = String;

    #[parse]
    const fn parse<'a, const N: usize>(
        text: &'a str,
        cx: &mut crate::attribute::ParseContext<'_>,
        parts: crate::attribute::ParseBuffer<'a, crate::attribute::TextPart<'a>, N>,
    ) -> Result<
        impl FnOnce(
            &mut crate::attribute::ResolveContext<'_>,
        ) -> Result<crate::Source<Self::Value>, String>
        + use<'a, N>,
        crate::attribute::ParseError,
    > {
        crate::attribute::template(text, cx, parts)
    }

    #[output(interface = "Display", name = "text")]
    fn display(value: Self::Value) -> String {
        value
    }
}
