use super::*;
use crate::geometry::Point;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextBounds {
    minimum: Point,
    maximum: Point,
}

impl TextBounds {
    pub fn new(minimum: Point, maximum: Point) -> Option<Self> {
        (minimum.is_finite()
            && maximum.is_finite()
            && minimum.x <= maximum.x
            && minimum.y <= maximum.y)
            .then_some(Self { minimum, maximum })
    }

    pub const fn minimum(self) -> Point {
        self.minimum
    }

    pub const fn maximum(self) -> Point {
        self.maximum
    }

    pub const fn width(self) -> f32 {
        self.maximum.x - self.minimum.x
    }

    pub const fn height(self) -> f32 {
        self.maximum.y - self.minimum.y
    }

    pub(in crate::text) fn union(self, other: Self) -> Self {
        Self {
            minimum: Point::new(
                self.minimum.x.min(other.minimum.x),
                self.minimum.y.min(other.minimum.y),
            ),
            maximum: Point::new(
                self.maximum.x.max(other.maximum.x),
                self.maximum.y.max(other.maximum.y),
            ),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextLineInfo {
    source_range: Range<usize>,
    baseline: Offset,
    ascent: f32,
    descent: f32,
    leading: f32,
    typographic_bounds: TextBounds,
}

impl TextLineInfo {
    pub fn source_range(&self) -> Range<usize> {
        self.source_range.clone()
    }

    pub const fn baseline(&self) -> Offset {
        self.baseline
    }

    pub const fn ascent(&self) -> f32 {
        self.ascent
    }

    pub const fn descent(&self) -> f32 {
        self.descent
    }

    pub const fn leading(&self) -> f32 {
        self.leading
    }

    pub const fn typographic_bounds(&self) -> TextBounds {
        self.typographic_bounds
    }
}

/// One writing-mode-independent line placed in physical text coordinates.
#[derive(Clone, Debug)]
pub(in crate::text) struct PositionedTextLine {
    pub(in crate::text) line: TextLine,
    pub(in crate::text) baseline: Offset,
    pub(in crate::text) source_range: Range<usize>,
    pub(in crate::text) metrics: LineMetrics,
}

#[derive(Clone, Debug)]
pub(in crate::text) struct PaintRange {
    pub(in crate::text) source_range: Range<usize>,
    pub(in crate::text) paint: Option<TextPaint>,
}

/// Vertical alignment within the text layout height.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextVerticalAlignment {
    #[default]
    Top,
    Center,
    Bottom,
}

/// Alignment along the text flow direction, independent of writing mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InlineAlignment {
    #[default]
    Start,
    Center,
    End,
    Justify,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WritingMode {
    Horizontal,
    Vertical,
}

/// Text with glyphs and lines resolved into physical two-dimensional positions.
///
/// The result remains semantic until [`Text::coverage`] lowers its glyphs to
/// Canvas paths. It intentionally contains no horizontal-layout configuration,
/// allowing another writing-mode layout to produce the same result type later.
#[derive(Clone, Debug)]
pub struct Text {
    pub(in crate::text) content: Arc<str>,
    pub(in crate::text) lines: Arc<[PositionedTextLine]>,
    pub(super) width: f32,
    pub(super) height: f32,
    pub(in crate::text) writing_mode: WritingMode,
    pub(in crate::text) paint_ranges: Arc<[PaintRange]>,
}

impl Text {
    /// Begins constructing horizontally laid-out text.
    ///
    /// Explicit hard breaks start new lines. Soft wrapping is disabled until
    /// [`HorizontalText::wrap_width`] is specified.
    pub fn horizontal(style: TextStyle) -> HorizontalText {
        HorizontalText::new(style)
    }

    /// Begins constructing top-to-bottom vertical text whose columns progress
    /// from right to left.
    pub fn vertical(style: TextStyle) -> VerticalText {
        VerticalText::new(style)
    }

    pub const fn writing_mode(&self) -> WritingMode {
        self.writing_mode
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Returns the logical UTF-8 source range represented by each visual line.
    pub fn line_ranges(&self) -> impl ExactSizeIterator<Item = Range<usize>> + '_ {
        self.lines
            .iter()
            .map(|positioned| positioned.source_range.clone())
    }

    /// Returns stable, renderer-independent placement information for each
    /// visual line or vertical column.
    pub fn lines(&self) -> impl ExactSizeIterator<Item = TextLineInfo> + '_ {
        self.lines
            .iter()
            .map(|positioned| positioned.info(self.writing_mode))
    }

    /// Returns the union of all typographic line boxes. This can extend beyond
    /// the requested layout size because layout does not imply clipping.
    pub fn typographic_bounds(&self) -> Option<TextBounds> {
        self.lines()
            .map(|line| line.typographic_bounds)
            .reduce(TextBounds::union)
    }

    pub fn has_typographic_overflow(&self) -> bool {
        self.typographic_bounds().is_some_and(|bounds| {
            bounds.minimum.x < 0.0
                || bounds.minimum.y < 0.0
                || bounds.maximum.x > self.width
                || bounds.maximum.y > self.height
        })
    }

    /// Returns logical paragraph ranges without their hard break characters.
    pub fn paragraph_ranges(&self) -> impl ExactSizeIterator<Item = Range<usize>> + '_ {
        super::line_breaking::paragraph_ranges(&self.content).into_iter()
    }

    /// Returns the ink-independent typographic layout width in Canvas units.
    ///
    /// Horizontal text keeps its requested wrap width even when its content is shorter.
    /// Unbreakable content may draw beyond that width; layout does not imply
    /// clipping.
    pub const fn width(&self) -> f32 {
        self.width
    }

    /// Returns the ink-independent typographic layout height in Canvas units.
    ///
    /// Vertical text keeps its requested wrap height even when its content is shorter.
    /// Horizontal text keeps its explicitly requested height. Content may draw
    /// beyond the layout height; layout does not imply clipping.
    pub const fn height(&self) -> f32 {
        self.height
    }
}

impl PositionedTextLine {
    fn info(&self, writing_mode: WritingMode) -> TextLineInfo {
        let inline_end = self.baseline + self.line.advance;
        let typographic_bounds = match writing_mode {
            WritingMode::Horizontal => TextBounds {
                minimum: Point::new(
                    self.baseline.x.min(inline_end.x),
                    self.baseline.y - self.metrics.ascent,
                ),
                maximum: Point::new(
                    self.baseline.x.max(inline_end.x),
                    self.baseline.y + self.metrics.descent,
                ),
            },
            WritingMode::Vertical => TextBounds {
                minimum: Point::new(
                    self.baseline.x - self.metrics.ascent,
                    self.baseline.y.min(inline_end.y),
                ),
                maximum: Point::new(
                    self.baseline.x + self.metrics.descent,
                    self.baseline.y.max(inline_end.y),
                ),
            },
        };
        TextLineInfo {
            source_range: self.source_range.clone(),
            baseline: self.baseline,
            ascent: self.metrics.ascent,
            descent: self.metrics.descent,
            leading: self.metrics.leading,
            typographic_bounds,
        }
    }
}
