use super::line_breaking::{BreakOpportunity, HyphenationOptions, Hyphenator};
use super::*;
mod balanced;
mod line;
mod placement;
mod runs;
mod wrapping;

use placement::*;
use runs::*;
use wrapping::wrap_paragraph;

/// Configuration for laying out horizontal text.
#[derive(Clone)]
pub struct HorizontalText {
    default_style: TextStyle,
    spans: Vec<Span>,
    wrap_width: Option<f32>,
    height: Option<f32>,
    line_height: Option<f32>,
    tab_size: f32,
    tab_stops: TabStops,
    hyphenator: Option<Arc<dyn Hyphenator>>,
    hyphenation_options: HyphenationOptions,
    shaping_options: ShapingOptions,
    alignment: TextAlignment,
    vertical_alignment: TextVerticalAlignment,
    paragraph_style: ParagraphStyle,
    paragraph_styles: Vec<(usize, ParagraphStyle)>,
    maximum_lines: Option<usize>,
    overflow: TextOverflow,
    line_break_style: LineBreakStyle,
    overflow_wrap: OverflowWrap,
    justification_options: JustificationOptions,
    line_break_strategy: LineBreakStrategy,
    hanging_punctuation: HangingPunctuation,
}

impl std::fmt::Debug for HorizontalText {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HorizontalText")
            .field("default_style", &self.default_style)
            .field("spans", &self.spans)
            .field("wrap_width", &self.wrap_width)
            .field("height", &self.height)
            .field("line_height", &self.line_height)
            .field("tab_size", &self.tab_size)
            .field("has_hyphenator", &self.hyphenator.is_some())
            .field("shaping_options", &self.shaping_options)
            .field("alignment", &self.alignment)
            .field("vertical_alignment", &self.vertical_alignment)
            .finish()
    }
}

impl HorizontalText {
    pub(super) fn new(default_style: TextStyle) -> Self {
        Self {
            default_style,
            spans: Vec::new(),
            wrap_width: None,
            height: None,
            line_height: None,
            tab_size: 8.0,
            tab_stops: TabStops::default(),
            hyphenator: None,
            hyphenation_options: HyphenationOptions::default(),
            shaping_options: ShapingOptions::default(),
            alignment: TextAlignment::Start,
            vertical_alignment: TextVerticalAlignment::Top,
            paragraph_style: ParagraphStyle::default(),
            paragraph_styles: Vec::new(),
            maximum_lines: None,
            overflow: TextOverflow::Visible,
            line_break_style: LineBreakStyle::Normal,
            overflow_wrap: OverflowWrap::Normal,
            justification_options: JustificationOptions::default(),
            line_break_strategy: LineBreakStrategy::Greedy,
            hanging_punctuation: HangingPunctuation::None,
        }
    }

    pub fn with_shaping_options(mut self, options: ShapingOptions) -> Self {
        self.shaping_options = options;
        self
    }

    pub fn push(mut self, span: impl Into<Span>) -> Self {
        self.spans.push(span.into());
        self
    }

    pub fn extend<I, S>(mut self, spans: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<Span>,
    {
        self.spans.extend(spans.into_iter().map(Into::into));
        self
    }

    /// Enables greedy wrapping at Unicode line-break opportunities.
    pub fn wrap_width(mut self, width: f32) -> Self {
        self.wrap_width = Some(width);
        self
    }

    /// Sets the physical vertical layout extent without clipping its contents.
    pub fn with_height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    /// Uses a fixed baseline-to-baseline distance for adjacent lines.
    ///
    /// The first baseline and the final line's descent remain derived from font
    /// metrics. A value smaller than the glyph extents is permitted and may
    /// intentionally overlap lines.
    pub fn with_line_height(mut self, line_height: f32) -> Self {
        self.line_height = Some(line_height);
        self
    }

    /// Sets the repeating tab-stop interval as a multiple of the default
    /// style's U+0020 space advance.
    pub fn with_tab_size(mut self, tab_size: f32) -> Self {
        self.tab_size = tab_size;
        self
    }

    pub fn with_tab_stops(mut self, tab_stops: TabStops) -> Self {
        self.tab_stops = tab_stops;
        self
    }

    /// Enables language-aware hyphenation using an externally owned policy or
    /// dictionary adapter. Without a provider, only explicit soft hyphens are
    /// considered.
    pub fn with_hyphenator(mut self, hyphenator: impl Hyphenator + 'static) -> Self {
        self.hyphenator = Some(Arc::new(hyphenator));
        self
    }

    pub fn with_hyphenation_options(mut self, options: HyphenationOptions) -> Self {
        self.hyphenation_options = options;
        self
    }

    pub fn aligned(mut self, alignment: TextAlignment) -> Self {
        self.alignment = alignment;
        self
    }

    pub fn inline_aligned(mut self, alignment: InlineAlignment) -> Self {
        self.alignment = text_alignment(alignment);
        self
    }

    pub fn vertically_aligned(mut self, alignment: TextVerticalAlignment) -> Self {
        self.vertical_alignment = alignment;
        self
    }

    pub fn with_paragraph_style(mut self, style: ParagraphStyle) -> Self {
        self.paragraph_style = style;
        self
    }

    /// Limits visual lines. Logical content is retained for inspection and
    /// accessibility even when its tail is not realized.
    pub fn with_maximum_lines(mut self, maximum: usize) -> Self {
        self.maximum_lines = Some(maximum);
        self
    }

    pub fn with_overflow(mut self, overflow: TextOverflow) -> Self {
        self.overflow = overflow;
        self
    }

    pub fn with_line_break_style(mut self, style: LineBreakStyle) -> Self {
        self.line_break_style = style;
        self
    }

    pub fn with_overflow_wrap(mut self, overflow_wrap: OverflowWrap) -> Self {
        self.overflow_wrap = overflow_wrap;
        self
    }

    pub fn with_justification_options(mut self, options: JustificationOptions) -> Self {
        self.justification_options = options;
        self
    }

    pub fn with_line_break_strategy(mut self, strategy: LineBreakStrategy) -> Self {
        self.line_break_strategy = strategy;
        self
    }

    pub fn with_hanging_punctuation(mut self, policy: HangingPunctuation) -> Self {
        self.hanging_punctuation = policy;
        self
    }

    /// Overrides paragraph geometry by zero-based logical paragraph index.
    /// Later calls for the same paragraph replace earlier ones.
    pub fn with_paragraph_style_at(mut self, index: usize, style: ParagraphStyle) -> Self {
        if let Some((_, current)) = self
            .paragraph_styles
            .iter_mut()
            .find(|(paragraph, _)| *paragraph == index)
        {
            *current = style;
        } else {
            self.paragraph_styles.push((index, style));
        }
        self
    }

    /// Resolves shaping, wrapping and placement into a common text result.
    pub fn layout(self) -> Result<Text, TextError> {
        let attributed =
            AttributedText::from_spans(self.default_style, self.spans, WritingMode::Horizontal);
        validate_font_size(attributed.default_style.font_size())?;
        validate_letter_spacing(attributed.default_style.letter_spacing())?;
        validate_word_spacing(attributed.default_style.word_spacing())?;
        validate_baseline_shift(attributed.default_style.baseline_shift())?;
        for style_range in attributed.style_ranges.iter() {
            validate_font_size(style_range.style.font_size())?;
            validate_letter_spacing(style_range.style.letter_spacing())?;
            validate_word_spacing(style_range.style.word_spacing())?;
            validate_baseline_shift(style_range.style.baseline_shift())?;
            if style_range.style.fonts().is_empty() {
                return Err(ShapingError::NoFonts.into());
            }
        }
        if let Some(width) = self.wrap_width {
            if !width.is_finite() {
                return Err(TextError::NonFiniteMaxWidth);
            }
            if width < 0.0 {
                return Err(TextError::NegativeMaxWidth);
            }
        }
        if let Some(height) = self.height {
            if !height.is_finite() {
                return Err(TextError::NonFiniteHeight);
            }
            if height < 0.0 {
                return Err(TextError::NegativeHeight);
            }
        }
        if let Some(line_height) = self.line_height {
            if !line_height.is_finite() {
                return Err(TextError::NonFiniteLineHeight);
            }
            if line_height < 0.0 {
                return Err(TextError::NegativeLineHeight);
            }
        }
        validate_tab_size(self.tab_size)?;
        validate_paragraph_style(self.paragraph_style)?;
        for (_, style) in &self.paragraph_styles {
            validate_paragraph_style(*style)?;
        }
        validate_justification_options(self.justification_options)?;
        let tab_interval = if attributed.content.contains('\t') {
            resolve_tab_interval(
                &attributed.default_style,
                &self.shaping_options,
                self.tab_size,
            )?
        } else {
            0.0
        };
        let resolved_tabs = ResolvedTabStops::new(self.tab_stops.clone(), tab_interval);
        let max_width = self.wrap_width.unwrap_or(f32::INFINITY);
        let mut lines = Vec::new();
        for (paragraph_index, paragraph) in
            super::line_breaking::paragraph_ranges(&attributed.content)
                .into_iter()
                .enumerate()
        {
            let paragraph_style = self
                .paragraph_styles
                .iter()
                .find_map(|(index, style)| (*index == paragraph_index).then_some(*style))
                .unwrap_or(self.paragraph_style);
            let alignment = paragraph_style
                .alignment()
                .map(text_alignment)
                .unwrap_or(self.alignment);
            let wrap_context = WrapContext {
                attributed: &attributed,
                max_width,
                options: &self.shaping_options,
                tab_stops: &resolved_tabs,
                hyphenator: self.hyphenator.as_deref(),
                hyphenation_options: self.hyphenation_options,
                paragraph_style,
                line_break_style: self.line_break_style,
                overflow_wrap: self.overflow_wrap,
                strategy: self.line_break_strategy,
                hanging_punctuation: self.hanging_punctuation,
                alignment,
            };
            lines.extend(wrap_paragraph(&wrap_context, paragraph, paragraph_index)?);
        }
        if let Some(maximum) = self.maximum_lines
            && lines.len() > maximum
        {
            lines.truncate(maximum);
            if self.overflow == TextOverflow::Ellipsis
                && let Some(last) = lines.last_mut()
            {
                ellipsize_line(last, &attributed, &self.shaping_options, &resolved_tabs)?;
            }
        }
        if let Some(width) = self.wrap_width {
            for line in lines
                .iter_mut()
                .filter(|line| line.justify_allowed && line.alignment == TextAlignment::Justify)
            {
                let line_width = if line.line_box_width.is_finite() {
                    line.line_box_width
                } else {
                    width
                };
                line.line.justify_to_width(
                    &attributed.content,
                    line_width,
                    self.justification_options,
                );
            }
        }
        position_lines(&mut lines, &attributed.default_style);
        let mut content_size = layout_size(&lines, &attributed.default_style);
        if let Some(line_height) = self.line_height {
            apply_fixed_line_height(&mut lines, &mut content_size.y, line_height);
        }

        let width = self.wrap_width.unwrap_or(content_size.x);
        let height = self.height.unwrap_or(content_size.y);
        align_lines(&mut lines, width);
        let vertical_offset =
            vertical_alignment_offset(height, content_size.y, self.vertical_alignment);
        shift_lines_y(&mut lines, vertical_offset);
        let lines: Arc<[PositionedTextLine]> = lines
            .into_iter()
            .map(|line| {
                let metrics = text_line_metrics(&line.line)
                    .or_else(|| {
                        attributed.default_style.fonts().first().map(|font| {
                            font_line_metrics(&font.resolve(), attributed.default_style.font_size())
                        })
                    })
                    .unwrap_or_default();
                PositionedTextLine {
                    line: line.line,
                    baseline: line.baseline,
                    source_range: line.source_range,
                    metrics,
                }
            })
            .collect();

        Ok(Text {
            content: attributed.content,
            lines,
            size: Vector2::new(width, height),
            writing_mode: WritingMode::Horizontal,
            paint_ranges: attributed.paint_ranges,
        })
    }
}

#[derive(Clone, Copy)]
struct WrapContext<'a> {
    attributed: &'a AttributedText,
    max_width: f32,
    options: &'a ShapingOptions,
    tab_stops: &'a ResolvedTabStops,
    hyphenator: Option<&'a dyn Hyphenator>,
    hyphenation_options: HyphenationOptions,
    paragraph_style: ParagraphStyle,
    line_break_style: LineBreakStyle,
    overflow_wrap: OverflowWrap,
    strategy: LineBreakStrategy,
    hanging_punctuation: HangingPunctuation,
    alignment: TextAlignment,
}

/// One laid-out line and its baseline position in text coordinates.
#[derive(Clone, Debug)]
struct HorizontalLine {
    line: TextLine,
    baseline: Vector2,
    source_range: Range<usize>,
    paragraph_is_rtl: bool,
    justify_allowed: bool,
    line_box_start: f32,
    line_box_width: f32,
    paragraph_index: usize,
    paragraph_first: bool,
    paragraph_last: bool,
    paragraph_spacing_before: f32,
    paragraph_spacing_after: f32,
    paragraph_range: Range<usize>,
    inline_insets: f32,
    alignment: TextAlignment,
}

const fn text_alignment(alignment: InlineAlignment) -> TextAlignment {
    match alignment {
        InlineAlignment::Start => TextAlignment::Start,
        InlineAlignment::Center => TextAlignment::Center,
        InlineAlignment::End => TextAlignment::End,
        InlineAlignment::Justify => TextAlignment::Justify,
    }
}

/// Horizontal line alignment within the text layout width.
///
/// Start and End follow each paragraph's base direction, while Left and Right
/// refer to the physical sides of the layout regardless of bidi direction.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextAlignment {
    #[default]
    Start,
    Center,
    End,
    Left,
    Right,
    Justify,
}

#[cfg(test)]
mod tests;
