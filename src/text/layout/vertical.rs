use super::line_breaking::{BreakOpportunity, HyphenationOptions, Hyphenator};
use super::*;
mod attributed;
mod column;
mod line;
mod metrics;
mod runs;
mod wrapping;

use column::VerticalColumn;
use line::VerticalItem;
use metrics::*;
use runs::*;
use wrapping::*;

/// Configuration for laying out vertical text from top to bottom, with
/// successive columns placed from right to left.
#[derive(Clone)]
pub struct VerticalText {
    default_style: TextStyle,
    spans: Vec<Span>,
    wrap_height: Option<f32>,
    column_width: Option<f32>,
    tab_size: f32,
    tab_stops: TabStops,
    hyphenator: Option<Arc<dyn Hyphenator>>,
    hyphenation_options: HyphenationOptions,
    shaping_options: ShapingOptions,
    inline_alignment: InlineAlignment,
    column_direction: VerticalColumnDirection,
    maximum_columns: Option<usize>,
    overflow: TextOverflow,
    line_break_style: LineBreakStyle,
    overflow_wrap: OverflowWrap,
    justification_options: JustificationOptions,
    hanging_punctuation: HangingPunctuation,
    paragraph_style: ParagraphStyle,
    paragraph_styles: Vec<(usize, ParagraphStyle)>,
}

impl std::fmt::Debug for VerticalText {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("VerticalText")
            .field("default_style", &self.default_style)
            .field("spans", &self.spans)
            .field("wrap_height", &self.wrap_height)
            .field("column_width", &self.column_width)
            .field("tab_size", &self.tab_size)
            .field("has_hyphenator", &self.hyphenator.is_some())
            .field("shaping_options", &self.shaping_options)
            .field("inline_alignment", &self.inline_alignment)
            .finish()
    }
}

impl VerticalText {
    pub(super) fn new(default_style: TextStyle) -> Self {
        Self {
            default_style,
            spans: Vec::new(),
            wrap_height: None,
            column_width: None,
            tab_size: 8.0,
            tab_stops: TabStops::default(),
            hyphenator: None,
            hyphenation_options: HyphenationOptions::default(),
            shaping_options: ShapingOptions::default(),
            inline_alignment: InlineAlignment::Start,
            column_direction: VerticalColumnDirection::RightToLeft,
            maximum_columns: None,
            overflow: TextOverflow::Visible,
            line_break_style: LineBreakStyle::Normal,
            overflow_wrap: OverflowWrap::Normal,
            justification_options: JustificationOptions::default(),
            hanging_punctuation: HangingPunctuation::None,
            paragraph_style: ParagraphStyle::default(),
            paragraph_styles: Vec::new(),
        }
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

    /// Enables greedy column wrapping at Unicode line-break opportunities.
    pub fn wrap_height(mut self, height: f32) -> Self {
        self.wrap_height = Some(height);
        self
    }

    /// Uses a fixed baseline-to-baseline distance for adjacent columns.
    ///
    /// The outer edges remain derived from the first and last columns' font
    /// metrics. A value smaller than their extents is permitted and may
    /// intentionally overlap columns.
    pub fn with_column_width(mut self, column_width: f32) -> Self {
        self.column_width = Some(column_width);
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

    /// Enables language-aware hyphenation using an externally supplied policy
    /// or dictionary adapter.
    pub fn with_hyphenator(mut self, hyphenator: impl Hyphenator + 'static) -> Self {
        self.hyphenator = Some(Arc::new(hyphenator));
        self
    }

    pub fn with_hyphenation_options(mut self, options: HyphenationOptions) -> Self {
        self.hyphenation_options = options;
        self
    }

    pub fn with_shaping_options(mut self, options: ShapingOptions) -> Self {
        self.shaping_options = options;
        self
    }

    /// Aligns each column within the physical vertical layout extent.
    pub fn vertically_aligned(mut self, alignment: TextVerticalAlignment) -> Self {
        self.inline_alignment = match alignment {
            TextVerticalAlignment::Top => InlineAlignment::Start,
            TextVerticalAlignment::Center => InlineAlignment::Center,
            TextVerticalAlignment::Bottom => InlineAlignment::End,
        };
        self
    }

    pub fn inline_aligned(mut self, alignment: InlineAlignment) -> Self {
        self.inline_alignment = alignment;
        self
    }

    pub fn with_column_direction(mut self, direction: VerticalColumnDirection) -> Self {
        self.column_direction = direction;
        self
    }

    pub fn with_maximum_columns(mut self, maximum: usize) -> Self {
        self.maximum_columns = Some(maximum);
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

    pub fn with_hanging_punctuation(mut self, policy: HangingPunctuation) -> Self {
        self.hanging_punctuation = policy;
        self
    }

    pub fn with_paragraph_style(mut self, style: ParagraphStyle) -> Self {
        self.paragraph_style = style;
        self
    }

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

    pub fn layout(self) -> Result<Text, TextError> {
        let attributed =
            AttributedText::from_spans(self.default_style, self.spans, WritingMode::Vertical);
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
        if let Some(height) = self.wrap_height {
            if !height.is_finite() {
                return Err(TextError::NonFiniteHeight);
            }
            if height < 0.0 {
                return Err(TextError::NegativeHeight);
            }
        }
        if let Some(column_width) = self.column_width {
            if !column_width.is_finite() {
                return Err(TextError::NonFiniteColumnWidth);
            }
            if column_width < 0.0 {
                return Err(TextError::NegativeColumnWidth);
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

        let max_height = self.wrap_height.unwrap_or(f32::INFINITY);
        let mut columns = Vec::new();
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
            let alignment = paragraph_style.alignment().unwrap_or(self.inline_alignment);
            let wrap_context = VerticalWrapContext {
                attributed: &attributed,
                max_height,
                options: &self.shaping_options,
                tab_stops: &resolved_tabs,
                hyphenator: self.hyphenator.as_deref(),
                hyphenation_options: self.hyphenation_options,
                line_break_style: self.line_break_style,
                overflow_wrap: self.overflow_wrap,
                hanging_punctuation: self.hanging_punctuation,
                paragraph_style,
                alignment,
            };
            columns.extend(wrap_vertical_paragraph(&wrap_context, paragraph)?);
        }
        if let Some(maximum) = self.maximum_columns
            && columns.len() > maximum
        {
            columns.truncate(maximum);
            if self.overflow == TextOverflow::Ellipsis
                && let Some(last) = columns.last_mut()
            {
                ellipsize_column(
                    last,
                    &attributed,
                    &self.shaping_options,
                    &resolved_tabs,
                    max_height,
                )?;
            }
        }
        if self.wrap_height.is_some() {
            for column in columns.iter_mut().filter(|column| {
                column.justify_allowed && column.alignment == InlineAlignment::Justify
            }) {
                column.line.justify_to_height(
                    &attributed.content,
                    column.line_box_height,
                    self.justification_options,
                );
            }
        }

        let column_metrics: Vec<_> = columns
            .iter()
            .map(|column| vertical_column_metrics(&column.line, &attributed.default_style))
            .collect();
        let paragraph_gaps: f32 = columns
            .windows(2)
            .filter(|pair| pair[1].paragraph_first)
            .map(|pair| pair[0].paragraph_spacing_after + pair[1].paragraph_spacing_before)
            .sum();
        let width = vertical_layout_width(&column_metrics, self.column_width) + paragraph_gaps;
        let natural_height = columns
            .iter()
            .map(|column| column.line.advance().y.abs() + column.inline_insets)
            .fold(0.0, f32::max);
        let height = self.wrap_height.unwrap_or(natural_height);

        let mut baseline_x =
            column_metrics
                .first()
                .map_or(0.0, |metrics| match self.column_direction {
                    VerticalColumnDirection::RightToLeft => width - metrics.right,
                    VerticalColumnDirection::LeftToRight => metrics.left,
                });
        let mut previous_paragraph_spacing_after = 0.0;
        for (index, (column, metrics)) in columns
            .iter_mut()
            .zip(column_metrics.iter().copied())
            .enumerate()
        {
            if index > 0 {
                let previous = column_metrics[index - 1];
                let paragraph_gap = if column.paragraph_first {
                    previous_paragraph_spacing_after + column.paragraph_spacing_before
                } else {
                    0.0
                };
                match self.column_direction {
                    VerticalColumnDirection::RightToLeft => {
                        baseline_x -= self.column_width.unwrap_or(previous.left + metrics.right)
                            + paragraph_gap;
                    }
                    VerticalColumnDirection::LeftToRight => {
                        baseline_x += self.column_width.unwrap_or(previous.right + metrics.left)
                            + paragraph_gap;
                    }
                }
            }
            let content_height = column.line.advance().y.abs();
            column.baseline = Vector2::new(
                baseline_x,
                column.line_box_start
                    + inline_alignment_offset(
                        column.line_box_height,
                        content_height,
                        column.alignment,
                    ),
            );
            previous_paragraph_spacing_after = if column.paragraph_last {
                column.paragraph_spacing_after
            } else {
                0.0
            };
        }
        let lines = columns
            .into_iter()
            .zip(column_metrics)
            .map(|(column, metrics)| PositionedTextLine {
                line: column.line,
                baseline: column.baseline,
                source_range: column.source_range,
                metrics: LineMetrics {
                    ascent: metrics.left,
                    descent: metrics.right,
                    leading: 0.0,
                },
            })
            .collect();

        Ok(Text {
            content: attributed.content,
            lines,
            size: Vector2::new(width, height),
            writing_mode: WritingMode::Vertical,
            paint_ranges: attributed.paint_ranges,
        })
    }
}

#[derive(Clone, Copy)]
struct VerticalWrapContext<'a> {
    attributed: &'a AttributedText,
    max_height: f32,
    options: &'a ShapingOptions,
    tab_stops: &'a ResolvedTabStops,
    hyphenator: Option<&'a dyn Hyphenator>,
    hyphenation_options: HyphenationOptions,
    line_break_style: LineBreakStyle,
    overflow_wrap: OverflowWrap,
    hanging_punctuation: HangingPunctuation,
    paragraph_style: ParagraphStyle,
    alignment: InlineAlignment,
}

/// Physical progression of successive top-to-bottom columns.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VerticalColumnDirection {
    #[default]
    RightToLeft,
    LeftToRight,
}

#[cfg(test)]
mod tests;
