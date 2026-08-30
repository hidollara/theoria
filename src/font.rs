use std::sync::Arc;

use skrifa::{MetadataProvider, instance::Size};

use crate::geometry::Point;

mod baseline;
mod face;
mod instance;
mod metrics;
mod realization;
mod variation;

pub(crate) use baseline::{BaselineAxis, BaselineKind};
use face::FontFace;
pub(crate) use instance::ResolvedFont;
pub use metrics::FontMetrics;
pub(crate) use metrics::VerticalFontMetrics;
pub(crate) use realization::{GlyphPresentationError, GlyphRealizationRequest, RealizedGlyph};
use variation::VariationSetting;
pub use variation::{VariationAxis, VariationAxisTag};

pub(crate) use skrifa::GlyphId;

/// A configured use of an immutable font face in Theoria's text frontend.
///
/// User-space variation settings remain unresolved until shaping selects this
/// font. The resolved normalized location then travels with the resulting
/// glyph run so shaping and realization use the same instance.
#[derive(Clone)]
pub struct Font {
    face: Arc<FontFace>,
    variation_settings: Arc<[VariationSetting]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontError {
    InvalidFace,
    NonFiniteVariation,
}

impl std::fmt::Display for FontError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidFace => {
                formatter.write_str("font data does not contain the selected face")
            }
            Self::NonFiniteVariation => formatter.write_str("font variation values must be finite"),
        }
    }
}

impl std::error::Error for FontError {}

impl std::fmt::Debug for Font {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Font")
            .field("face", &self.face)
            .field("variation_settings", &self.variation_settings)
            .finish()
    }
}

impl Font {
    fn new(face: FontFace) -> Self {
        Self {
            face: Arc::new(face),
            variation_settings: Arc::from([]),
        }
    }

    /// Loads the first face from a supported font representation.
    ///
    /// Theoria detects the representation from the supplied bytes. Currently
    /// supported representations are decoded by the internal OpenType
    /// implementation.
    pub fn from_bytes(data: impl Into<Arc<[u8]>>) -> Result<Self, FontError> {
        FontFace::from_bytes(data).map(Self::new)
    }

    /// Loads one collection face and configures its default variation instance.
    pub fn from_collection_face(
        data: impl Into<Arc<[u8]>>,
        face_index: u32,
    ) -> Result<Self, FontError> {
        FontFace::from_collection_face(data, face_index).map(Self::new)
    }

    pub(crate) fn same_configuration(&self, other: &Self) -> bool {
        self.face.same_face(&other.face) && self.variation_settings == other.variation_settings
    }

    /// Returns the selected face's zero-based index in its source font data.
    pub fn face_index(&self) -> u32 {
        self.face.face_index()
    }
}

impl ResolvedFont {
    /// Resolves one baseline in the selected face's design-unit coordinates.
    ///
    /// OpenType BASE is optional. Layout supplies its writing-mode-specific
    /// fallback when a font does not declare the requested baseline.
    pub(crate) fn baseline_coordinate(
        &self,
        axis: BaselineAxis,
        script: Option<[u8; 4]>,
        baseline: BaselineKind,
    ) -> Option<f32> {
        baseline::coordinate(self.face(), self.location(), axis, script, baseline)
    }

    /// Returns the selected face's outline ink box in design units.
    ///
    /// OpenType requires intrinsic color presentations to have a corresponding
    /// outline glyph. That outline is the stable, presentation-independent
    /// bounds source used by Text inspection; a color presentation can have a
    /// slightly different static ink box.
    pub(crate) fn glyph_ink_bounds(&self, glyph: GlyphId) -> Option<[Point; 2]> {
        let bounds = self
            .face()
            .glyph_metrics(Size::unscaled(), self.location())
            .bounds(glyph)?;
        if bounds.x_min == bounds.x_max || bounds.y_min == bounds.y_max {
            return None;
        }
        Some([
            Point::new(bounds.x_min, bounds.y_min),
            Point::new(bounds.x_max, bounds.y_max),
        ])
    }
}

#[cfg(test)]
mod tests;
