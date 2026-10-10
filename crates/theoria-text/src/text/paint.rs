use std::sync::Arc;

use crate::canvas::ColorField;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextDecorationKind {
    Underline,
    Overline,
    LineThrough,
}

/// One decoration resolved only when laid-out text is lowered to Canvas.
#[derive(Clone, Debug)]
pub struct TextDecoration {
    kind: TextDecorationKind,
    source: Option<ColorField>,
    thickness: Option<f32>,
    offset: Option<f32>,
}

impl TextDecoration {
    pub const fn new(kind: TextDecorationKind) -> Self {
        Self {
            kind,
            source: None,
            thickness: None,
            offset: None,
        }
    }

    pub fn with_source(mut self, source: impl Into<ColorField>) -> Self {
        self.source = Some(source.into());
        self
    }

    pub const fn with_thickness(mut self, thickness: f32) -> Self {
        self.thickness = Some(thickness);
        self
    }

    pub const fn with_offset(mut self, offset: f32) -> Self {
        self.offset = Some(offset);
        self
    }

    pub const fn kind(&self) -> TextDecorationKind {
        self.kind
    }

    pub fn source(&self) -> Option<&ColorField> {
        self.source.as_ref()
    }

    pub const fn thickness(&self) -> Option<f32> {
        self.thickness
    }

    pub const fn offset(&self) -> Option<f32> {
        self.offset
    }
}

#[derive(Clone, Debug, Default)]
struct TextPaintData {
    foreground: Option<ColorField>,
    color_palette: u16,
    decorations: Vec<TextDecoration>,
}

/// Painting attributes for a span. These do not participate in shaping or
/// line breaking and are resolved at the Text-to-Canvas boundary.
#[derive(Clone, Debug, Default)]
pub(in crate::text) struct TextPaint(Arc<TextPaintData>);

impl TextPaint {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_foreground(mut self, foreground: impl Into<ColorField>) -> Self {
        Arc::make_mut(&mut self.0).foreground = Some(foreground.into());
        self
    }

    /// Selects the CPAL palette used while realizing intrinsic color glyphs.
    /// A missing palette falls back to the font's default palette.
    pub fn with_color_palette(mut self, palette_index: u16) -> Self {
        Arc::make_mut(&mut self.0).color_palette = palette_index;
        self
    }

    pub fn with_decoration(mut self, decoration: TextDecoration) -> Self {
        Arc::make_mut(&mut self.0).decorations.push(decoration);
        self
    }

    pub fn foreground(&self) -> Option<&ColorField> {
        self.0.foreground.as_ref()
    }

    pub fn color_palette(&self) -> u16 {
        self.0.color_palette
    }

    pub fn decorations(&self) -> &[TextDecoration] {
        &self.0.decorations
    }

    pub(super) fn same_identity(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    pub(super) fn is_empty(&self) -> bool {
        self.0.foreground.is_none() && self.0.color_palette == 0 && self.0.decorations.is_empty()
    }
}
