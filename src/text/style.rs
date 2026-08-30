use std::sync::Arc;

use super::OpenTypeFeature;
use super::paint::{TextDecoration, TextPaint};
use crate::canvas::ColorField;
use crate::font::Font;

#[derive(Clone, Debug)]
struct TextStyleData {
    fonts: Arc<[Font]>,
    font_size: f32,
    letter_spacing: f32,
    word_spacing: f32,
    language: Option<String>,
    features: Vec<OpenTypeFeature>,
    vertical_orientation: VerticalOrientation,
    baseline_shift: f32,
    paint: TextPaint,
}

/// Writing-mode-neutral typography applied to a text span.
#[derive(Clone, Debug)]
pub struct TextStyle(Arc<TextStyleData>);

impl TextStyle {
    /// Creates a resolved text style with an ordered font fallback chain.
    ///
    /// For each extended grapheme cluster, layout selects the first font that
    /// can shape the complete cluster without producing a missing glyph. No
    /// implicit platform-font lookup is performed; applications that want a
    /// last-resort font should place it at the end of `fonts`.
    pub fn new(fonts: &[Font], font_size: f32) -> Self {
        Self(Arc::new(TextStyleData {
            fonts: fonts.into(),
            font_size,
            letter_spacing: 0.0,
            word_spacing: 0.0,
            language: None,
            features: Vec::new(),
            vertical_orientation: VerticalOrientation::Mixed,
            baseline_shift: 0.0,
            paint: TextPaint::new(),
        }))
    }

    pub fn with_font_size(mut self, font_size: f32) -> Self {
        Arc::make_mut(&mut self.0).font_size = font_size;
        self
    }

    /// Replaces the ordered font fallback chain.
    pub fn with_fonts(mut self, fonts: &[Font]) -> Self {
        Arc::make_mut(&mut self.0).fonts = fonts.into();
        self
    }

    /// Adds tracking between adjacent typographic character units in Canvas
    /// units. Negative values tighten text.
    pub fn with_letter_spacing(mut self, letter_spacing: f32) -> Self {
        Arc::make_mut(&mut self.0).letter_spacing = letter_spacing;
        self
    }

    /// Adds space around explicit word-separator characters in Canvas units.
    /// This does not perform linguistic word segmentation. Negative values
    /// tighten the intrinsic separator advance.
    pub fn with_word_spacing(mut self, word_spacing: f32) -> Self {
        Arc::make_mut(&mut self.0).word_spacing = word_spacing;
        self
    }

    /// Sets a BCP 47 language tag such as `"en"` or `"ja"`.
    pub fn with_language(mut self, language: impl Into<String>) -> Self {
        let language = language.into();
        Arc::make_mut(&mut self.0).language = (!language.is_empty()).then_some(language);
        self
    }

    pub fn with_feature(mut self, feature: OpenTypeFeature) -> Self {
        Arc::make_mut(&mut self.0).features.push(feature);
        self
    }

    pub fn with_vertical_orientation(mut self, orientation: VerticalOrientation) -> Self {
        Arc::make_mut(&mut self.0).vertical_orientation = orientation;
        self
    }

    /// Moves the span toward the typographic ascent side without changing the
    /// paragraph baseline. Positive values raise horizontal text.
    pub fn with_baseline_shift(mut self, shift: f32) -> Self {
        Arc::make_mut(&mut self.0).baseline_shift = shift;
        self
    }

    pub fn with_foreground(mut self, foreground: impl Into<ColorField>) -> Self {
        let data = Arc::make_mut(&mut self.0);
        data.paint = data.paint.clone().with_foreground(foreground);
        self
    }

    /// Selects the CPAL palette used while realizing intrinsic color glyphs.
    /// A missing palette falls back to the font's default palette.
    pub fn with_color_palette(mut self, palette_index: u16) -> Self {
        let data = Arc::make_mut(&mut self.0);
        data.paint = data.paint.clone().with_color_palette(palette_index);
        self
    }

    pub fn with_decoration(mut self, decoration: TextDecoration) -> Self {
        let data = Arc::make_mut(&mut self.0);
        data.paint = data.paint.clone().with_decoration(decoration);
        self
    }

    pub fn fonts(&self) -> &[Font] {
        &self.0.fonts
    }

    pub fn font_size(&self) -> f32 {
        self.0.font_size
    }

    pub fn letter_spacing(&self) -> f32 {
        self.0.letter_spacing
    }

    pub fn word_spacing(&self) -> f32 {
        self.0.word_spacing
    }

    pub fn language(&self) -> Option<&str> {
        self.0.language.as_deref()
    }

    pub fn features(&self) -> &[OpenTypeFeature] {
        &self.0.features
    }

    pub fn vertical_orientation(&self) -> VerticalOrientation {
        self.0.vertical_orientation
    }

    pub fn baseline_shift(&self) -> f32 {
        self.0.baseline_shift
    }

    pub fn foreground(&self) -> Option<&ColorField> {
        self.0.paint.foreground()
    }

    pub fn color_palette(&self) -> u16 {
        self.0.paint.color_palette()
    }

    pub fn decorations(&self) -> &[TextDecoration] {
        self.0.paint.decorations()
    }

    pub(super) fn paint(&self) -> Option<&TextPaint> {
        (!self.0.paint.is_empty()).then_some(&self.0.paint)
    }

    pub(super) fn same_layout_attributes(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
            || (self.0.font_size == other.0.font_size
                && self.0.letter_spacing == other.0.letter_spacing
                && self.0.word_spacing == other.0.word_spacing
                && self.0.language == other.0.language
                && self.0.features == other.0.features
                && self.0.vertical_orientation == other.0.vertical_orientation
                && self.0.baseline_shift == other.0.baseline_shift
                && self.0.fonts.len() == other.0.fonts.len()
                && self
                    .0
                    .fonts
                    .iter()
                    .zip(other.0.fonts.iter())
                    .all(|(left, right)| left.same_configuration(right)))
    }
}

/// One writing-mode-neutral text fragment with an optional style override.
#[derive(Clone, Debug)]
pub struct Span {
    content: Arc<str>,
    style: Option<TextStyle>,
}

impl Span {
    pub fn new(content: impl Into<Arc<str>>) -> Self {
        Self {
            content: content.into(),
            style: None,
        }
    }

    pub fn with_style(mut self, style: TextStyle) -> Self {
        self.style = Some(style);
        self
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn style(&self) -> Option<&TextStyle> {
        self.style.as_ref()
    }

    pub(super) fn into_parts(self) -> (Arc<str>, Option<TextStyle>) {
        (self.content, self.style)
    }
}

impl From<&str> for Span {
    fn from(content: &str) -> Self {
        Self::new(content)
    }
}

impl From<String> for Span {
    fn from(content: String) -> Self {
        Self::new(content)
    }
}

impl From<Arc<str>> for Span {
    fn from(content: Arc<str>) -> Self {
        Self::new(content)
    }
}

/// Orientation policy for one span in vertical writing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VerticalOrientation {
    /// Uses Unicode Vertical_Orientation: CJK is generally upright while
    /// horizontal scripts are generally rotated clockwise.
    #[default]
    Mixed,
    /// Keeps every typographic character unit upright.
    Upright,
    /// Shapes text horizontally and rotates it clockwise into the column.
    Sideways,
    /// Shapes the span horizontally and places it upright inside one em-square
    /// cell, compressing it horizontally when necessary.
    Combined,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::OpenTypeFeatureTag;

    #[test]
    fn spans_are_writing_mode_neutral_fragments_with_optional_typography() {
        let default_style = TextStyle::new(&[], 16.0).with_language("en");
        let emphasized = default_style
            .clone()
            .with_font_size(24.0)
            .with_feature(OpenTypeFeatureTag::STANDARD_LIGATURES.disabled());
        let plain = Span::new("plain");
        let styled = Span::new("styled").with_style(emphasized.clone());

        assert_eq!(default_style.font_size(), 16.0);
        assert_eq!(default_style.letter_spacing(), 0.0);
        assert_eq!(default_style.word_spacing(), 0.0);
        assert_eq!(default_style.language(), Some("en"));
        assert!(default_style.fonts().is_empty());
        assert!(default_style.features().is_empty());
        assert_eq!(plain.content(), "plain");
        assert!(plain.style().is_none());
        assert_eq!(styled.content(), "styled");
        assert_eq!(styled.style().unwrap().font_size(), 24.0);
        assert_eq!(emphasized.features().len(), 1);
    }

    #[test]
    fn open_type_features_keep_their_builtin_identity_and_value() {
        let setting = OpenTypeFeatureTag::STANDARD_LIGATURES.disabled();

        assert_eq!(setting.tag(), OpenTypeFeatureTag::STANDARD_LIGATURES);
        assert_eq!(setting.tag().bytes(), *b"liga");
        assert_eq!(setting.value(), 0);
    }
}
