use skrifa::{
    GlyphId, MetadataProvider, instance::Size, outline::DrawSettings, raw::TableProvider,
};

mod bitmap;
mod colr;
mod outline;
mod svg;

use crate::{
    canvas::{Canvas, ColorField},
    path::Path,
};

use self::outline::GlyphPathBuilder;
use super::ResolvedFont;

/// A glyph presentation lowered to Theoria's semantic drawing types.
#[derive(Clone, Debug)]
pub(crate) enum RealizedGlyph {
    Outline(Path),
    Intrinsic(Canvas),
    Empty,
    Unsupported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GlyphPresentationError {
    MissingOutline,
    InvalidColorGlyph,
    UnsupportedColorPaint,
    InvalidBitmap,
    UnsupportedSvg,
}

/// Parameters that affect presentation but not font instance identity.
pub(crate) struct GlyphRealizationRequest<'a> {
    pub font_size: f32,
    pub palette_index: u16,
    pub foreground: &'a ColorField,
}

/// A temporary query for one glyph in an immutable font instance.
#[derive(Clone, Copy)]
pub(crate) struct GlyphRef<'a> {
    font: &'a ResolvedFont,
    id: GlyphId,
}

impl ResolvedFont {
    pub(crate) const fn glyph_ref(&self, id: GlyphId) -> GlyphRef<'_> {
        GlyphRef { font: self, id }
    }
}

impl GlyphRef<'_> {
    /// Realizes only the scalable monochrome outline presentation.
    pub(crate) fn outline(self) -> RealizedGlyph {
        let Ok(path) = self.outline_path() else {
            return RealizedGlyph::Unsupported;
        };
        if path.is_empty() {
            RealizedGlyph::Empty
        } else {
            RealizedGlyph::Outline(path)
        }
    }

    /// Realizes the first usable intrinsic presentation in COLR, SVG,
    /// bitmap, then outline order.
    ///
    /// Font fallback has completed before this boundary. Failure therefore
    /// does not silently select another font and invalidate shaping metrics.
    pub(crate) fn realize(
        self,
        request: &GlyphRealizationRequest<'_>,
    ) -> Result<RealizedGlyph, GlyphPresentationError> {
        let mut intrinsic_error = None;
        match colr::canvas(
            self.font,
            self.id,
            request.palette_index,
            request.foreground,
        ) {
            Ok(Some(canvas)) => return Ok(RealizedGlyph::Intrinsic(canvas)),
            Ok(None) => {}
            Err(error) => intrinsic_error = Some(error),
        }

        if let Some(data) = self
            .font
            .face()
            .svg()
            .ok()
            .and_then(|svg| svg.glyph_data(self.id).ok().flatten())
        {
            match svg::canvas(
                self.font,
                data,
                self.id,
                request.font_size,
                request.palette_index,
                request.foreground,
            ) {
                Ok(canvas) => return Ok(RealizedGlyph::Intrinsic(canvas)),
                Err(error) => intrinsic_error.get_or_insert(error),
            };
        }

        if let Some(bitmap) = self
            .font
            .face()
            .bitmap_strikes()
            .glyph_for_size(Size::new(request.font_size), self.id)
        {
            match bitmap::canvas(
                bitmap,
                self.font.metrics().units_per_em(),
                request.foreground,
            ) {
                Ok(canvas) => return Ok(RealizedGlyph::Intrinsic(canvas)),
                Err(error) => intrinsic_error.get_or_insert(error),
            };
        }

        match self.outline() {
            RealizedGlyph::Unsupported => {
                Err(intrinsic_error.unwrap_or(GlyphPresentationError::MissingOutline))
            }
            fallback => Ok(fallback),
        }
    }

    pub(super) fn outline_path(self) -> Result<Path, GlyphPresentationError> {
        let mut builder = GlyphPathBuilder::default();
        let outline = self
            .font
            .face()
            .outline_glyphs()
            .get(self.id)
            .ok_or(GlyphPresentationError::MissingOutline)?;
        outline
            .draw(
                DrawSettings::unhinted(Size::unscaled(), self.font.location()),
                &mut builder,
            )
            .map_err(|_| GlyphPresentationError::MissingOutline)?;
        Ok(builder.finish())
    }
}
