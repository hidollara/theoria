use crate::{
    canvas::{Canvas, ColorField, Composite, Coverage},
    geometry::Point,
    math::Vector2,
    transform::Affine2,
};

mod decoration;
mod glyphs;
mod range_geometry;

use super::layout::{Text, TextBounds};
use super::{TextError, TextPaint};
use decoration::decoration_coverage;
use glyphs::{GlyphRealizationError, glyph_placement};
use range_geometry::range_regions;

impl From<GlyphRealizationError> for TextError {
    fn from(error: GlyphRealizationError) -> Self {
        match error {
            GlyphRealizationError::UnsupportedGlyph { glyph_index } => {
                Self::UnsupportedGlyph { glyph_index }
            }
            GlyphRealizationError::InvalidGlyphPresentation { glyph_index } => {
                Self::UnsupportedGlyph { glyph_index }
            }
        }
    }
}

impl Text {
    /// Returns the union of the positioned glyph ink boxes.
    ///
    /// The bounds are independent of clipping and can therefore extend beyond
    /// [`Text::size`]. Intrinsic color glyphs use their corresponding OpenType
    /// outline box, which is the stable box available without rasterizing the
    /// presentation merely to inspect layout.
    pub fn ink_bounds(&self) -> Option<TextBounds> {
        self.lines
            .iter()
            .flat_map(|line| {
                let line_placement = Affine2::from_translation(line.baseline);
                line.line.runs.iter().flat_map(move |positioned| {
                    let run_placement = positioned.placement.then(line_placement);
                    let scale = positioned.run.font_size
                        / f32::from(positioned.run.font.metrics().units_per_em());
                    let mut pen = Vector2::ZERO;
                    positioned.run.glyphs.iter().filter_map(move |glyph| {
                        let origin = pen + glyph.offset;
                        pen = pen + glyph.advance;
                        let [minimum, maximum] =
                            positioned.run.font.glyph_ink_bounds(glyph.glyph_id)?;
                        let placement = glyph_placement(scale, origin).then(run_placement);
                        transformed_bounds(minimum, maximum, placement)
                    })
                })
            })
            .reduce(TextBounds::union)
    }

    /// Lowers the positioned monochrome glyphs into generic Canvas coverage.
    pub fn coverage(&self) -> Result<Coverage, TextError> {
        self.coverage_for_range(None)
    }

    fn coverage_for_range(
        &self,
        source_range: Option<&std::ops::Range<usize>>,
    ) -> Result<Coverage, TextError> {
        let mut mask = Canvas::empty();
        for positioned in self.lines.iter() {
            let line_mask = positioned
                .line
                .coverage_canvas(source_range)?
                .transformed(Affine2::from_translation(positioned.baseline));
            mask = mask.composite(line_mask, Composite::SourceOver);
        }
        Ok(Coverage::from_alpha(mask))
    }

    fn realized_canvas_for_range(
        &self,
        foreground: &ColorField,
        palette_index: u16,
        source_range: Option<&std::ops::Range<usize>>,
    ) -> Result<Canvas, TextError> {
        let mut canvas = Canvas::empty();
        for positioned in self.lines.iter() {
            let line = positioned
                .line
                .realized_canvas(foreground, palette_index, source_range)?
                .transformed(Affine2::from_translation(positioned.baseline));
            canvas = canvas.composite(line, Composite::SourceOver);
        }
        Ok(canvas)
    }

    /// Draws the realized glyph coverage using an arbitrary Canvas color field.
    pub fn to_canvas(&self, foreground: impl Into<ColorField>) -> Result<Canvas, TextError> {
        let foreground = foreground.into();
        if self.paint_ranges.iter().all(|range| range.paint.is_none()) {
            return self.realized_canvas_for_range(&foreground, 0, None);
        }

        let mut decorations = Canvas::empty();
        let mut glyphs = Canvas::empty();
        for range in self.paint_ranges.iter() {
            let paint = range.paint.as_ref();
            let source = paint
                .and_then(TextPaint::foreground)
                .cloned()
                .unwrap_or_else(|| foreground.clone());
            let palette_index = paint.map_or(0, TextPaint::color_palette);
            if let Some(paint) = paint {
                for decoration in paint.decorations() {
                    let decoration_source = decoration
                        .source()
                        .cloned()
                        .unwrap_or_else(|| source.clone());
                    for region in range_regions(self, range.source_range.clone()) {
                        let coverage = decoration_coverage(region, self.writing_mode, decoration)?;
                        decorations = decorations.composite(
                            Canvas::draw(decoration_source.clone(), coverage),
                            Composite::SourceOver,
                        );
                    }
                }
            }
            glyphs = glyphs.composite(
                self.realized_canvas_for_range(&source, palette_index, Some(&range.source_range))?,
                Composite::SourceOver,
            );
        }
        Ok(decorations.composite(glyphs, Composite::SourceOver))
    }
}

fn transformed_bounds(minimum: Point, maximum: Point, transform: Affine2) -> Option<TextBounds> {
    let corners = [
        minimum,
        Point::new(maximum.x, minimum.y),
        maximum,
        Point::new(minimum.x, maximum.y),
    ];
    let mut transformed = corners
        .into_iter()
        .map(|point| transform.transform_point(point));
    let first = transformed.next()?;
    let (minimum, maximum) = transformed.fold((first, first), |(minimum, maximum), point| {
        (
            Point::new(minimum.x.min(point.x), minimum.y.min(point.y)),
            Point::new(maximum.x.max(point.x), maximum.y.max(point.y)),
        )
    });
    TextBounds::new(minimum, maximum)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_reports_positioned_glyph_ink_bounds() {
        use crate::text::{TextStyle, test_support::bundled_latin_font};

        let font = bundled_latin_font();
        let text = Text::horizontal(TextStyle::new(&[font], 32.0))
            .push("Ink")
            .layout()
            .unwrap();
        let ink = text.ink_bounds().expect("visible glyphs have ink");

        assert!(ink.width() > 0.0);
        assert!(ink.height() > 0.0);
        assert!(text.typographic_bounds().unwrap().height() >= ink.height());
    }

    #[test]
    fn painted_spans_lower_foregrounds_and_decorations_only_at_canvas_boundary() {
        use crate::{
            color::Color,
            text::{
                Span, TextDecoration, TextDecorationKind, TextStyle,
                test_support::bundled_latin_font,
            },
        };

        let font = bundled_latin_font();
        let painted = TextStyle::new(std::slice::from_ref(&font), 24.0)
            .with_foreground(Color::rgb8(220, 40, 70))
            .with_decoration(TextDecoration::new(TextDecorationKind::Underline));
        let text = Text::horizontal(TextStyle::new(&[font], 24.0))
            .push("plain ")
            .push(Span::new("styled").with_style(painted))
            .layout()
            .unwrap();

        let canvas = text.to_canvas(Color::rgb8(20, 20, 20)).unwrap();
        assert!(matches!(
            canvas.node(),
            crate::canvas::CanvasNode::Composite { .. }
        ));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn installed_color_font_realizes_intrinsic_glyph_paint() {
        use crate::color::Color;

        let Ok(data) = std::fs::read(r"C:\Windows\Fonts\seguiemj.ttf") else {
            return;
        };
        let font = crate::font::Font::from_bytes(data).unwrap();
        let text = Text::horizontal(crate::text::TextStyle::new(&[font], 48.0))
            .push("😀")
            .layout()
            .unwrap();

        assert!(text.to_canvas(Color::rgb8(0, 0, 0)).is_ok());
    }
}
