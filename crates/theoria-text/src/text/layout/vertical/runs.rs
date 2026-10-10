use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
struct VerticalOrientationSegment {
    range: Range<usize>,
    orientation: SegmentOrientation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SegmentOrientation {
    Upright,
    Sideways,
    TransformedRotated,
    Combined,
}

fn vertical_orientation_segments(
    text: &str,
    policy: VerticalOrientation,
) -> Vec<VerticalOrientationSegment> {
    let mut segments: Vec<VerticalOrientationSegment> = Vec::new();
    for (start, grapheme) in grapheme_indices(text) {
        let end = start + grapheme.len();
        let orientation = match policy {
            VerticalOrientation::Upright => SegmentOrientation::Upright,
            VerticalOrientation::Sideways => SegmentOrientation::Sideways,
            VerticalOrientation::Combined => SegmentOrientation::Combined,
            VerticalOrientation::Mixed => match vertical_glyph_orientation(grapheme) {
                VerticalGlyphOrientation::Upright
                | VerticalGlyphOrientation::TransformedUpright => SegmentOrientation::Upright,
                VerticalGlyphOrientation::Rotated => SegmentOrientation::Sideways,
                VerticalGlyphOrientation::TransformedRotated => {
                    SegmentOrientation::TransformedRotated
                }
            },
        };
        if let Some(previous) = segments.last_mut()
            && previous.orientation == orientation
            && orientation != SegmentOrientation::TransformedRotated
        {
            previous.range.end = end;
        } else {
            segments.push(VerticalOrientationSegment {
                range: start..end,
                orientation,
            });
        }
    }
    segments
}

pub(super) fn shape_vertical_range(
    attributed: &AttributedText,
    source_range: Range<usize>,
    context: &ShapingOptions,
    tab_stops: &ResolvedTabStops,
) -> Result<TextLine, ShapingError> {
    if source_range.is_empty() {
        return Ok(TextLine::default());
    }

    let paragraph = super::super::line_breaking::paragraph_ranges(&attributed.content)
        .into_iter()
        .find(|paragraph| {
            paragraph.start <= source_range.start && source_range.end <= paragraph.end
        })
        .unwrap_or_else(|| source_range.clone());
    let paragraph_text = attributed.bidi_text(paragraph.clone());
    let bidi = bidi_info(&paragraph_text, paragraph_level(context));
    let local_line = source_range.start - paragraph.start..source_range.end - paragraph.start;
    let (_, visual_segments) = bidi.visual_runs(local_line);

    let mut items = Vec::new();
    for visual_segment in visual_segments {
        if visual_segment.is_empty() {
            continue;
        }
        let direction = if bidi.levels[visual_segment.start].is_rtl() {
            HorizontalDirection::RightToLeft
        } else {
            HorizontalDirection::LeftToRight
        };
        let source_segment =
            paragraph.start + visual_segment.start..paragraph.start + visual_segment.end;
        let mut style_ranges = attributed.style_ranges_within(source_segment);
        if direction == HorizontalDirection::RightToLeft {
            style_ranges.reverse();
        }
        for style_range in style_ranges {
            shape_vertical_style_range(
                attributed,
                style_range,
                direction,
                context,
                tab_stops,
                &mut items,
            )?;
        }
    }
    Ok(TextLine::from_vertical_items(items, &attributed.content))
}

fn shape_vertical_style_range(
    attributed: &AttributedText,
    style_range: StyleRange,
    bidi_direction: HorizontalDirection,
    context: &ShapingOptions,
    tab_stops: &ResolvedTabStops,
    items: &mut Vec<VerticalItem>,
) -> Result<(), ShapingError> {
    let text = &attributed.content[style_range.source_range.clone()];
    if style_range.style.vertical_orientation() == VerticalOrientation::Combined {
        let options = style_range
            .style
            .shaping_options(context)
            .direction(bidi_direction);
        let runs = shape_spaced_segment(
            &style_range.style,
            text,
            &options,
            style_range.source_range.start,
            &attributed.content,
            style_range.spacing_group,
            tab_stops,
        )?;
        let mut combined = Vec::new();
        for run in runs {
            if run.is_tab() {
                if !combined.is_empty() {
                    items.push(VerticalItem::Combined {
                        runs: std::mem::take(&mut combined),
                        em_size: style_range.style.font_size(),
                    });
                }
                items.push(VerticalItem::Oriented(run, RunOrientation::Sideways));
            } else {
                combined.push(run);
            }
        }
        if !combined.is_empty() {
            items.push(VerticalItem::Combined {
                runs: combined,
                em_size: style_range.style.font_size(),
            });
        }
        return Ok(());
    }
    for segment in vertical_orientation_segments(text, style_range.style.vertical_orientation()) {
        let segment_text = &text[segment.range.clone()];
        let mut options = style_range.style.shaping_options(context);
        if segment.orientation == SegmentOrientation::Upright {
            options = options.exact_direction(ShapingDirection::TopToBottom);
        } else if segment.orientation == SegmentOrientation::Sideways {
            options = options.direction(bidi_direction);
        }
        let source_offset = style_range.source_range.start + segment.range.start;
        let (runs, run_orientation) =
            if segment.orientation == SegmentOrientation::TransformedRotated {
                transformed_rotated_runs(
                    attributed,
                    &style_range,
                    segment_text,
                    bidi_direction,
                    context,
                    source_offset,
                    tab_stops,
                )?
            } else {
                (
                    shape_spaced_segment(
                        &style_range.style,
                        segment_text,
                        &options,
                        source_offset,
                        &attributed.content,
                        style_range.spacing_group,
                        tab_stops,
                    )?,
                    match segment.orientation {
                        SegmentOrientation::Upright => RunOrientation::Upright,
                        SegmentOrientation::Sideways => RunOrientation::Sideways,
                        SegmentOrientation::Combined | SegmentOrientation::TransformedRotated => {
                            unreachable!()
                        }
                    },
                )
            };
        items.extend(runs.into_iter().map(|run| {
            let orientation = if run.is_tab() {
                RunOrientation::Sideways
            } else {
                run_orientation
            };
            VerticalItem::Oriented(run, orientation)
        }));
    }
    Ok(())
}

fn transformed_rotated_runs(
    attributed: &AttributedText,
    style_range: &StyleRange,
    text: &str,
    bidi_direction: HorizontalDirection,
    context: &ShapingOptions,
    source_offset: usize,
    tab_stops: &ResolvedTabStops,
) -> Result<(Vec<InlineRun>, RunOrientation), ShapingError> {
    let vertical = shape_spaced_segment(
        &style_range.style,
        text,
        &style_range
            .style
            .shaping_options(context)
            .exact_direction(ShapingDirection::TopToBottom),
        source_offset,
        &attributed.content,
        style_range.spacing_group,
        tab_stops,
    )?;
    let horizontal = shape_spaced_segment(
        &style_range.style,
        text,
        &style_range
            .style
            .shaping_options(context)
            .direction(bidi_direction),
        source_offset,
        &attributed.content,
        style_range.spacing_group,
        tab_stops,
    )?;
    if !same_glyph_identities(&vertical, &horizontal) {
        Ok((vertical, RunOrientation::Upright))
    } else {
        Ok((horizontal, RunOrientation::Sideways))
    }
}

fn same_glyph_identities(left: &[InlineRun], right: &[InlineRun]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| left.has_same_glyph_identities(right))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixed_vertical_orientation_uses_unicode_vertical_orientation_data() {
        assert_eq!(
            vertical_orientation_segments("A本", VerticalOrientation::Mixed),
            [
                VerticalOrientationSegment {
                    range: 0..1,
                    orientation: SegmentOrientation::Sideways,
                },
                VerticalOrientationSegment {
                    range: 1..4,
                    orientation: SegmentOrientation::Upright,
                },
            ]
        );
        assert_eq!(
            vertical_orientation_segments("A本", VerticalOrientation::Upright),
            [VerticalOrientationSegment {
                range: 0..4,
                orientation: SegmentOrientation::Upright,
            }]
        );
    }

    #[test]
    fn explicit_upright_text_is_strong_ltr_only_for_bidi_resolution() {
        let source = "Aאב\u{10000}";
        let upright = AttributedText::from_spans(
            TextStyle::new(&[], 16.0).with_vertical_orientation(VerticalOrientation::Upright),
            vec![Span::new(source)],
            WritingMode::Vertical,
        );
        let bidi_text = upright.bidi_text(0..source.len());

        assert_eq!(bidi_text.len(), source.len());
        assert!(
            bidi_info(&bidi_text, None)
                .levels
                .iter()
                .all(|level| level.is_ltr())
        );
        assert_eq!(upright.content.as_ref(), source);

        let mixed = AttributedText::from_spans(
            TextStyle::new(&[], 16.0).with_vertical_orientation(VerticalOrientation::Mixed),
            vec![Span::new(source)],
            WritingMode::Vertical,
        );
        assert!(
            bidi_info(&mixed.bidi_text(0..source.len()), None)
                .levels
                .iter()
                .any(|level| level.is_rtl())
        );
    }
}
