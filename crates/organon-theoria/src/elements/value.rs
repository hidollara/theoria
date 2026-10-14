//! Scalar and string attribute interpretation.

use crate::implement;

#[implement]
impl f32 {
    type Value = f32;

    #[parse]
    fn parse(
        text: &str,
        cx: &mut crate::attribute::ParseContext<'_>,
    ) -> Result<crate::Source<Self::Value>, String> {
        crate::attribute::reference_or_literal::<Self>(text, cx, |text| {
            text.parse::<f32>()
                .ok()
                .filter(|v| v.is_finite())
                .ok_or("scalar must be finite")
        })
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
    fn parse(
        text: &str,
        cx: &mut crate::attribute::ParseContext<'_>,
    ) -> Result<crate::Source<Self::Value>, String> {
        crate::attribute::template(text, cx)
    }

    #[output(interface = "Display", name = "text")]
    fn display(value: Self::Value) -> String {
        value
    }
}
