use std::{ops::Range, sync::Arc};

use crate::{
    font::{GlyphId, ResolvedFont},
    math::Vector2,
};

#[cfg(test)]
use crate::font::Font;

use super::{
    GlyphRunError, ShapingDirection, ShapingOptions,
    harfrust_backend::{resolved_direction, shape_glyphs},
};

/// One shaper result in the font's native design-unit coordinate system.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::text) struct ShapedGlyph {
    pub(in crate::text) glyph_id: GlyphId,
    pub(in crate::text) cluster: usize,
    pub(in crate::text) offset: Vector2,
    pub(in crate::text) advance: Vector2,
    /// HarfRust found that U+0640 can be inserted before this cluster without
    /// invalidating the surrounding shaping result.
    pub(in crate::text) safe_to_insert_tatweel: bool,
}

impl ShapedGlyph {
    pub(super) const fn new(
        glyph_id: GlyphId,
        cluster: usize,
        offset: Vector2,
        advance: Vector2,
        safe_to_insert_tatweel: bool,
    ) -> Self {
        Self {
            glyph_id,
            cluster,
            offset,
            advance,
            safe_to_insert_tatweel,
        }
    }
}

/// A single-font sequence produced by shaping one text segment.
#[derive(Clone, Debug)]
pub(in crate::text) struct GlyphRun {
    pub(in crate::text) font: ResolvedFont,
    pub(in crate::text) glyphs: Arc<[ShapedGlyph]>,
    pub(in crate::text) source_range: Range<usize>,
    pub(in crate::text) direction: ShapingDirection,
    pub(super) script: Option<[u8; 4]>,
    pub(super) options: ShapingOptions,
}

impl GlyphRun {
    pub(super) fn new(
        font: ResolvedFont,
        glyphs: impl IntoIterator<Item = ShapedGlyph>,
        source_range: Range<usize>,
        direction: ShapingDirection,
        script: Option<[u8; 4]>,
        options: ShapingOptions,
    ) -> Result<Self, GlyphRunError> {
        let glyphs: Arc<[ShapedGlyph]> = glyphs.into_iter().collect();
        validate_placements(&glyphs)?;
        Ok(Self {
            font,
            glyphs,
            source_range,
            direction,
            script,
            options,
        })
    }

    #[cfg(test)]
    pub(super) fn shape(font: Font, text: &str) -> Result<Self, GlyphRunError> {
        Self::shape_with(font, text, &ShapingOptions::default())
    }

    #[cfg(test)]
    pub(in crate::text) fn shape_with(
        font: Font,
        text: &str,
        options: &ShapingOptions,
    ) -> Result<Self, GlyphRunError> {
        Self::shape_with_context(font.resolve(), text, options, None)
    }

    pub(super) fn shape_with_context(
        font: ResolvedFont,
        text: &str,
        options: &ShapingOptions,
        context: Option<(&str, Range<usize>)>,
    ) -> Result<Self, GlyphRunError> {
        let glyphs = shape_glyphs(&font, text, options, context);
        Self::new(
            font,
            glyphs,
            0..text.len(),
            resolved_direction(text, options),
            options.script,
            options.clone(),
        )
    }

    pub(super) fn with_source_offset(mut self, source_offset: usize) -> Self {
        let glyphs = Arc::make_mut(&mut self.glyphs);
        for glyph in glyphs {
            glyph.cluster += source_offset;
        }
        self.source_range.start += source_offset;
        self.source_range.end += source_offset;
        self
    }

    pub(in crate::text) const fn font(&self) -> &ResolvedFont {
        &self.font
    }

    pub(in crate::text) const fn script(&self) -> Option<[u8; 4]> {
        self.script
    }

    #[cfg(test)]
    pub(super) fn glyph_count(&self) -> usize {
        self.glyphs.len()
    }

    #[cfg(test)]
    pub(super) fn clusters(&self) -> impl ExactSizeIterator<Item = usize> + '_ {
        self.glyphs.iter().map(|glyph| glyph.cluster)
    }

    pub(in crate::text) fn advance(&self) -> Vector2 {
        self.glyphs
            .iter()
            .fold(Vector2::ZERO, |sum, glyph| sum + glyph.advance)
    }
}

pub(super) fn validate_placements(glyphs: &[ShapedGlyph]) -> Result<(), GlyphRunError> {
    if glyphs
        .iter()
        .any(|glyph| !is_finite(glyph.offset) || !is_finite(glyph.advance))
    {
        return Err(GlyphRunError::NonFinitePlacement);
    }
    Ok(())
}

fn is_finite(vector: Vector2) -> bool {
    vector.x.is_finite() && vector.y.is_finite()
}
