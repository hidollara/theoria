use std::{ops::Range, sync::Arc};

use super::line_breaking::HyphenationBoundary;
use super::{
    HyphenationOptions, Hyphenator, PaintRange, ShapingOptions, Span, TextPaint, TextStyle,
    VerticalOrientation, WritingMode,
};

#[derive(Clone, Debug)]
pub(super) struct StyleRange {
    pub(super) source_range: Range<usize>,
    pub(super) style: TextStyle,
    pub(super) spacing_group: usize,
}

#[derive(Clone, Debug)]
pub(super) struct AttributedText {
    pub(super) content: Arc<str>,
    pub(super) style_ranges: Arc<[StyleRange]>,
    pub(super) paint_ranges: Arc<[PaintRange]>,
    pub(super) default_style: TextStyle,
    writing_mode: WritingMode,
}

impl AttributedText {
    pub(super) fn from_spans(
        default_style: TextStyle,
        spans: Vec<Span>,
        writing_mode: WritingMode,
    ) -> Self {
        let mut content = String::new();
        let mut style_ranges: Vec<StyleRange> = Vec::new();
        let mut paint_ranges: Vec<PaintRange> = Vec::new();
        let mut next_spacing_group = 0;
        for span in spans {
            let (span_content, span_style) = span.into_parts();
            if span_content.is_empty() {
                continue;
            }
            let start = content.len();
            content.push_str(&span_content);
            let end = content.len();
            let style = span_style.unwrap_or_else(|| default_style.clone());
            let paint = style.paint();
            if let Some(previous) = paint_ranges.last_mut()
                && previous.source_range.end == start
                && same_paint(previous.paint.as_ref(), paint)
            {
                previous.source_range.end = end;
            } else {
                paint_ranges.push(PaintRange {
                    source_range: start..end,
                    paint: paint.cloned(),
                });
            }

            let preserves_span_boundary = matches!(writing_mode, WritingMode::Vertical)
                && style.vertical_orientation() == VerticalOrientation::Combined;
            if let Some(previous) = style_ranges.last_mut()
                && previous.source_range.end == start
                && previous.style.same_layout_attributes(&style)
                && !preserves_span_boundary
            {
                previous.source_range.end = end;
            } else {
                let spacing_group = next_spacing_group;
                next_spacing_group += 1;
                style_ranges.push(StyleRange {
                    source_range: start..end,
                    style,
                    spacing_group,
                });
            }
        }
        Self {
            content: content.into(),
            style_ranges: style_ranges.into(),
            paint_ranges: paint_ranges.into(),
            default_style,
            writing_mode,
        }
    }

    pub(super) fn style_ranges_within(&self, source_range: Range<usize>) -> Vec<StyleRange> {
        self.style_ranges
            .iter()
            .filter_map(|style_range| {
                let start = source_range.start.max(style_range.source_range.start);
                let end = source_range.end.min(style_range.source_range.end);
                (start < end).then(|| StyleRange {
                    source_range: start..end,
                    style: style_range.style.clone(),
                    spacing_group: style_range.spacing_group,
                })
            })
            .collect()
    }

    pub(super) fn hyphenation_boundaries(
        &self,
        paragraph: Range<usize>,
        context: &ShapingOptions,
        hyphenator: &dyn Hyphenator,
        options: HyphenationOptions,
    ) -> Vec<HyphenationBoundary> {
        let mut language_ranges: Vec<(Range<usize>, Option<String>)> = Vec::new();
        for style_range in self.style_ranges_within(paragraph.clone()) {
            let suppresses_hyphenation = matches!(self.writing_mode, WritingMode::Vertical)
                && style_range.style.vertical_orientation() == VerticalOrientation::Combined;
            let language = (!suppresses_hyphenation)
                .then(|| {
                    style_range
                        .style
                        .language()
                        .or(context.language.as_deref())
                        .map(str::to_owned)
                })
                .flatten();
            if let Some((range, previous_language)) = language_ranges.last_mut()
                && range.end == style_range.source_range.start
                && *previous_language == language
            {
                range.end = style_range.source_range.end;
            } else {
                language_ranges.push((style_range.source_range, language));
            }
        }

        let mut boundaries = Vec::new();
        for (range, language) in language_ranges {
            let Some(language) = language else {
                continue;
            };
            boundaries.extend(
                super::line_breaking::provider_boundaries(
                    &self.content[range.clone()],
                    &language,
                    hyphenator,
                    options,
                )
                .into_iter()
                .map(|boundary| HyphenationBoundary {
                    index: range.start - paragraph.start + boundary.index,
                    show_hyphen: boundary.show_hyphen,
                    automatic: boundary.automatic,
                }),
            );
        }
        boundaries
    }
}

fn same_paint(left: Option<&TextPaint>, right: Option<&TextPaint>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => left.same_identity(right),
        _ => false,
    }
}
