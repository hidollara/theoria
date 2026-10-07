use std::ops::Range;

use crate::{
    canvas::{Canvas, Color, ColorField, Coverage, Opacity},
    color::Srgb,
    font::{GlyphPresentationError, GlyphRealizationRequest, RealizedGlyph},
    geometry::Offset,
    transform::Affine2,
};

use super::super::layout::{LayoutRun, TextLine};
use super::overlay;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GlyphRealizationError {
    UnsupportedGlyph { glyph_index: u32 },
    InvalidGlyphPresentation { glyph_index: u32 },
}

impl std::fmt::Display for GlyphRealizationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedGlyph { glyph_index } => {
                write!(
                    formatter,
                    "glyph {glyph_index} has no supported presentation"
                )
            }
            Self::InvalidGlyphPresentation { glyph_index } => {
                write!(
                    formatter,
                    "glyph {glyph_index} has an unsupported or invalid presentation"
                )
            }
        }
    }
}

impl std::error::Error for GlyphRealizationError {}

impl LayoutRun {
    /// Lowers monochrome glyph outlines into a generic Canvas alpha field.
    ///
    /// The run begins at baseline origin `(0, 0)`. Each outline becomes an
    /// ordinary Path draw, preserving the separation between glyph shape and
    /// the eventual foreground color field.
    pub(super) fn coverage_canvas(
        &self,
        source_range: Option<&Range<usize>>,
    ) -> Result<Option<Canvas>, GlyphRealizationError> {
        if self.font_size == 0.0 || self.glyphs.is_empty() {
            return Ok(None);
        }

        let scale = self.font_size / f32::from(self.font.metrics().units_per_em());
        let mut pen = Offset::ZERO;
        let mut mask = None;

        for positioned in self.glyphs.iter().copied() {
            if source_range.is_some_and(|range| !range.contains(&positioned.cluster)) {
                pen = pen + positioned.advance;
                continue;
            }
            match self.font.glyph_ref(positioned.glyph_id).outline() {
                RealizedGlyph::Outline(path) => {
                    let placement = glyph_placement(scale, pen + positioned.offset);
                    let glyph_mask = Canvas::draw(
                        Color::new(Srgb, [255_u8, 255, 255], Opacity::<u8>::ONE),
                        Coverage::fill(path),
                    )
                    .transformed(placement);
                    mask = overlay(mask, Some(glyph_mask));
                }
                RealizedGlyph::Empty => {}
                RealizedGlyph::Intrinsic(_) => {
                    unreachable!("outline lookup never returns intrinsic color")
                }
                RealizedGlyph::Unsupported => {
                    return Err(GlyphRealizationError::UnsupportedGlyph {
                        glyph_index: positioned.glyph_id.to_u32(),
                    });
                }
            }
            pen = pen + positioned.advance;
        }

        Ok(mask)
    }

    pub(super) fn realized_canvas(
        &self,
        foreground: &ColorField,
        palette_index: u16,
        source_range: Option<&Range<usize>>,
    ) -> Result<Option<Canvas>, GlyphRealizationError> {
        if self.font_size == 0.0 || self.glyphs.is_empty() {
            return Ok(None);
        }

        let scale = self.font_size / f32::from(self.font.metrics().units_per_em());
        let mut pen = Offset::ZERO;
        let mut canvas = None;
        for positioned in self.glyphs.iter().copied() {
            if source_range.is_some_and(|range| !range.contains(&positioned.cluster)) {
                pen = pen + positioned.advance;
                continue;
            }
            let glyph = self
                .font
                .glyph_ref(positioned.glyph_id)
                .realize(&GlyphRealizationRequest {
                    font_size: self.font_size,
                    palette_index,
                    foreground,
                })
                .map_err(|_error: GlyphPresentationError| {
                    GlyphRealizationError::InvalidGlyphPresentation {
                        glyph_index: positioned.glyph_id.to_u32(),
                    }
                })?;
            let placement = glyph_placement(scale, pen + positioned.offset);
            let glyph_canvas = match glyph {
                RealizedGlyph::Outline(path) => {
                    Canvas::draw(foreground.clone(), Coverage::fill(path))
                }
                RealizedGlyph::Intrinsic(canvas) => canvas,
                RealizedGlyph::Empty => {
                    pen = pen + positioned.advance;
                    continue;
                }
                RealizedGlyph::Unsupported => {
                    return Err(GlyphRealizationError::UnsupportedGlyph {
                        glyph_index: positioned.glyph_id.to_u32(),
                    });
                }
            }
            .transformed(placement);
            canvas = overlay(canvas, Some(glyph_canvas));
            pen = pen + positioned.advance;
        }
        Ok(canvas)
    }
}

impl TextLine {
    pub(super) fn coverage_canvas(
        &self,
        source_range: Option<&Range<usize>>,
    ) -> Result<Option<Canvas>, GlyphRealizationError> {
        let mut mask = None;
        for positioned in self.runs.iter() {
            let run_mask = positioned
                .run
                .coverage_canvas(source_range)?
                .map(|canvas| canvas.transformed(positioned.placement));
            mask = overlay(mask, run_mask);
        }
        Ok(mask)
    }

    pub(super) fn realized_canvas(
        &self,
        foreground: &ColorField,
        palette_index: u16,
        source_range: Option<&Range<usize>>,
    ) -> Result<Option<Canvas>, GlyphRealizationError> {
        let mut canvas = None;
        for positioned in self.runs.iter() {
            let run = positioned
                .run
                .realized_canvas(foreground, palette_index, source_range)?
                .map(|canvas| canvas.transformed(positioned.placement));
            canvas = overlay(canvas, run);
        }
        Ok(canvas)
    }
}

pub(super) fn glyph_placement(scale: f32, origin: Offset) -> Affine2 {
    Affine2::from_translation(origin)
        .expect("finite translation")
        .then(Affine2::from_scale(scale, -scale).expect("finite scale"))
        .expect("finite composed transform")
}

#[cfg(test)]
mod tests {
    use crate::geometry::{Offset, Point};

    use super::*;

    #[test]
    fn glyph_placement_scales_design_units_and_flips_font_y() {
        let transform = glyph_placement(0.02, Offset::new(500.0, 250.0));

        assert_eq!(
            transform.transform_point(Point::new(0.0, 0.0)),
            Point::new(10.0, -5.0)
        );
        assert_eq!(
            transform.transform_point(Point::new(100.0, 200.0)),
            Point::new(12.0, -9.0)
        );
    }
}
