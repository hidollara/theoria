//! Coverage, canvas construction, and composition.

use super::color::Space;
use crate::implement;
use theoria_canvas::{
    Canvas, ColorField, Composite, Composition, Coverage,
    color::{DisplayP3, LinearDisplayP3, LinearSrgb, Srgb},
    geometry::Offset,
    transform::Affine2,
};

#[implement]
impl Canvas {
    type Value = Canvas;

    #[output(name = "field", interface = "ColorSource")]
    fn canvas_field(value: Canvas) -> ColorField {
        value.into()
    }
}

#[implement]
impl Coverage {
    type Value = Coverage;
}

#[implement]
impl All {
    #[output(name = "coverage")]
    fn all() -> Result<Coverage, &'static str> {
        Ok(Coverage::all())
    }
}

#[implement]
impl Fill {
    #[output(name = "coverage")]
    fn fill(
        input: Input<'_, Self>,
        cx: &crate::ReadContext<'_, '_>,
    ) -> Result<Coverage, &'static str> {
        Ok(Coverage::fill(
            (*input.children()[0].path().read(cx)).clone()?,
        ))
    }
}

#[implement]
impl Draw {
    #[output(name = "drawing")]
    fn draw(
        input: Input<'_, Self>,
        cx: &crate::ReadContext<'_, '_>,
    ) -> Result<Canvas, &'static str> {
        let source = input.source();
        let children = input.children();
        Ok(Canvas::draw(
            source.field().read(cx).into_cloned(),
            (*children[0].coverage().read(cx)).clone()?,
        ))
    }
}

#[implement]
impl Drawing {
    forward!(canvas => drawing);
}

#[implement]
impl Translate {
    #[output(name = "drawing")]
    fn translate(
        input: Input<'_, Self>,
        cx: &crate::ReadContext<'_, '_>,
    ) -> Result<Canvas, &'static str> {
        let x = input.x().into_cloned();
        let y = input.y().into_cloned();
        let children = input.children().into_cloned();

        Ok((*children[0].canvas().read(cx)).clone()?.transformed(
            Affine2::from_translation(Offset::new(x, y))
                .ok_or("Translate requires finite offsets")?,
        ))
    }
}

#[implement]
impl SourceOver {
    #[output(name = "drawing")]
    fn source_over(
        input: Input<'_, Self>,
        cx: &crate::ReadContext<'_, '_>,
    ) -> Result<Canvas, &'static str> {
        let space = input.space().into_cloned();
        let children = input.children().into_cloned();

        let backdrop = (*children[0].canvas().read(cx)).clone()?;
        let source = (*children[1].canvas().read(cx)).clone()?;
        Ok(match space {
            Space::Srgb => {
                backdrop.composite(source, Composition::new(Composite::SourceOver, Srgb))
            }
            Space::LinearSrgb => {
                backdrop.composite(source, Composition::new(Composite::SourceOver, LinearSrgb))
            }
            Space::DisplayP3 => {
                backdrop.composite(source, Composition::new(Composite::SourceOver, DisplayP3))
            }
            Space::LinearDisplayP3 => backdrop.composite(
                source,
                Composition::new(Composite::SourceOver, LinearDisplayP3),
            ),
        })
    }
}
