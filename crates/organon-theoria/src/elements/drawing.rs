//! Coverage, canvas construction, and composition.

use super::color::Space;
use crate::implement;
use theoria_canvas::{
    Canvas, Color, ColorField, Composite, Composition, Coverage, Opacity,
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
    #[output(name = "coverage", interface = "CoverageSource")]
    fn all() -> Result<Coverage, &'static str> {
        Ok(Coverage::all())
    }
}

#[implement]
impl Fill {
    #[output(name = "coverage")]
    #[output(name = "coverage", interface = "CoverageSource")]
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
    #[output(name = "canvas", interface = "Drawable")]
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
    forward!(canvas => Drawable.canvas);
}

#[implement]
impl Translate {
    #[output(name = "drawing")]
    #[output(name = "canvas", interface = "Drawable")]
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
    #[output(name = "canvas", interface = "Drawable")]
    fn source_over(
        input: Input<'_, Self>,
        cx: &crate::ReadContext<'_, '_>,
    ) -> Result<Canvas, &'static str> {
        let mut children = input.children().into_cloned().into_iter();
        let Some(first) = children.next() else {
            return Ok(Canvas::draw(
                Color::new(Srgb, [0u8; 3], Opacity::<u8>::from(0)),
                Coverage::all(),
            ));
        };
        let mut drawing = first.canvas().read(cx).into_cloned()?;
        if children.len() == 0 {
            return Ok(drawing);
        }
        let composition = match input.space().into_cloned() {
            Space::Srgb => Composition::new(Composite::SourceOver, Srgb),
            Space::LinearSrgb => Composition::new(Composite::SourceOver, LinearSrgb),
            Space::DisplayP3 => Composition::new(Composite::SourceOver, DisplayP3),
            Space::LinearDisplayP3 => Composition::new(Composite::SourceOver, LinearDisplayP3),
        };
        for child in children {
            drawing =
                drawing.composite(child.canvas().read(cx).into_cloned()?, composition.clone());
        }
        Ok(drawing)
    }
}
