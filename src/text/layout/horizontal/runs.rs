use super::*;

#[cfg(test)]
fn shape_isolated_line(
    fonts: &[Font],
    font_size: f32,
    text: &str,
    options: &ShapingOptions,
) -> Result<TextLine, ShapingError> {
    let style = TextStyle::new(fonts, font_size);
    let attributed =
        AttributedText::from_spans(style, vec![Span::new(text)], WritingMode::Horizontal);
    let bidi = bidi_info(text, paragraph_level(options));
    let tabs = ResolvedTabStops::new(TabStops::default(), 8.0);
    shape_bidi_line(
        &attributed,
        0..text.len(),
        0..text.len(),
        &bidi,
        options,
        &tabs,
    )
}

pub(super) fn shape_bidi_line(
    attributed: &AttributedText,
    paragraph: Range<usize>,
    line: Range<usize>,
    bidi: &ParagraphBidiInfo<'_>,
    context: &ShapingOptions,
    tab_stops: &ResolvedTabStops,
) -> Result<TextLine, ShapingError> {
    if line.is_empty() {
        return Ok(TextLine::default());
    }

    let (_, visual_segments) = bidi.visual_runs(line);
    let mut runs = Vec::new();
    for segment in visual_segments {
        if segment.is_empty() {
            continue;
        }
        let direction = if bidi.levels[segment.start].is_rtl() {
            HorizontalDirection::RightToLeft
        } else {
            HorizontalDirection::LeftToRight
        };
        let source_segment = paragraph.start + segment.start..paragraph.start + segment.end;
        let mut style_ranges = attributed.style_ranges_within(source_segment);
        if direction == HorizontalDirection::RightToLeft {
            style_ranges.reverse();
        }
        for style_range in style_ranges {
            let segment_options = style_range
                .style
                .shaping_options(context)
                .direction(direction);
            let shaped = shape_spaced_segment(
                &style_range.style,
                &attributed.content[style_range.source_range.clone()],
                &segment_options,
                style_range.source_range.start,
                &attributed.content,
                style_range.spacing_group,
                tab_stops,
            )?;
            runs.extend(shaped);
        }
    }
    Ok(TextLine::from_runs(runs, &attributed.content))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::test_support::windows_test_font;
    use crate::{canvas::Opacity, color::Srgb};

    #[cfg(target_os = "windows")]
    #[test]
    fn a_text_line_resolves_mixed_horizontal_directions_before_shaping() {
        let Some(font) = windows_test_font() else {
            return;
        };
        let line = shape_isolated_line(
            &[font],
            24.0,
            "abc \u{05d0}\u{05d1}\u{05d2} def",
            &ShapingOptions::default(),
        )
        .unwrap();

        assert_eq!(line.runs.len(), 3);
        assert!(line.advance().x > 0.0);
        assert!(
            line.runs
                .windows(2)
                .all(|runs| runs[0].inline_start < runs[1].inline_start)
        );
    }

    #[test]
    fn adjacent_fragments_with_the_same_style_remain_one_shaping_range() {
        let style = TextStyle::new(&[], 16.0);
        let attributed = AttributedText::from_spans(
            style,
            vec![Span::new("f"), Span::new("f"), Span::new("i")],
            WritingMode::Horizontal,
        );

        assert_eq!(&*attributed.content, "ffi");
        assert_eq!(attributed.style_ranges.len(), 1);
        assert_eq!(attributed.style_ranges[0].source_range, 0..3);
    }

    #[test]
    fn an_explicit_style_change_creates_a_shaping_boundary() {
        let default_style = TextStyle::new(&[], 16.0);
        let larger = default_style.clone().with_font_size(24.0);
        let attributed = AttributedText::from_spans(
            default_style,
            vec![Span::new("ab"), Span::new("cd").with_style(larger)],
            WritingMode::Horizontal,
        );

        assert_eq!(&*attributed.content, "abcd");
        assert_eq!(attributed.style_ranges.len(), 2);
        assert_eq!(attributed.style_ranges[0].source_range, 0..2);
        assert_eq!(attributed.style_ranges[1].source_range, 2..4);
        assert_eq!(attributed.style_ranges[0].style.font_size(), 16.0);
        assert_eq!(attributed.style_ranges[1].style.font_size(), 24.0);
    }

    #[test]
    fn paint_boundaries_are_retained_without_becoming_canvas_state() {
        let typography = TextStyle::new(&[], 16.0);
        let painted = typography
            .clone()
            .with_foreground(crate::canvas::Color::new(
                Srgb,
                [255_u8, 0, 0],
                Opacity::<u8>::ONE,
            ));
        let attributed = AttributedText::from_spans(
            typography,
            vec![Span::new("a"), Span::new("b").with_style(painted)],
            WritingMode::Horizontal,
        );
        assert_eq!(attributed.style_ranges.len(), 1);
        assert_eq!(attributed.paint_ranges.len(), 2);
        assert!(attributed.paint_ranges[0].paint.is_none());
        assert!(attributed.paint_ranges[1].paint.is_some());
    }
}
