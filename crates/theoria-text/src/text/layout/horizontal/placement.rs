use super::*;

pub(super) fn position_lines(lines: &mut [HorizontalLine], default_style: &TextStyle) {
    let fallback = default_style
        .fonts()
        .first()
        .map(|font| font_line_metrics(&font.resolve(), default_style.font_size()));
    let mut previous_baseline = 0.0;
    let mut previous_metrics = None;
    let mut previous_paragraph_spacing_after = 0.0;
    for positioned in lines {
        let metrics = text_line_metrics(&positioned.line)
            .or(fallback)
            .unwrap_or_default();
        let new_paragraph = positioned.paragraph_first && positioned.paragraph_index > 0;
        let paragraph_gap = if new_paragraph {
            previous_paragraph_spacing_after + positioned.paragraph_spacing_before
        } else {
            0.0
        };
        let baseline_y = previous_metrics.map_or(metrics.ascent, |previous: LineMetrics| {
            previous_baseline + previous.descent + previous.leading + metrics.ascent + paragraph_gap
        });
        positioned.baseline = Offset::new(0.0, baseline_y);
        previous_baseline = baseline_y;
        previous_metrics = Some(metrics);
        previous_paragraph_spacing_after = if positioned.paragraph_last {
            positioned.paragraph_spacing_after
        } else {
            0.0
        };
    }
}

pub(super) fn align_lines(lines: &mut [HorizontalLine], width: f32) {
    for positioned in lines {
        let alignment = positioned.alignment;
        let box_width = if positioned.line_box_width.is_finite() {
            positioned.line_box_width
        } else {
            width - positioned.line_box_start
        };
        let remaining = box_width - positioned.line.advance().x.abs();
        positioned.baseline.x = positioned.line_box_start
            + match alignment {
                TextAlignment::Center => remaining * 0.5,
                TextAlignment::Left => 0.0,
                TextAlignment::Right => remaining,
                TextAlignment::Start if positioned.paragraph_is_rtl => remaining,
                TextAlignment::End if !positioned.paragraph_is_rtl => remaining,
                TextAlignment::Start | TextAlignment::End => 0.0,
                TextAlignment::Justify => 0.0,
            };
    }
}

pub(super) fn shift_lines_y(lines: &mut [HorizontalLine], offset: f32) {
    for line in lines {
        line.baseline.y += offset;
    }
}

pub(super) fn apply_fixed_line_height(
    lines: &mut [HorizontalLine],
    content_height: &mut f32,
    line_height: f32,
) {
    let Some(first_baseline) = lines.first().map(|line| line.baseline.y) else {
        return;
    };
    let final_descent = lines
        .last()
        .map_or(0.0, |line| *content_height - line.baseline.y);
    let mut baseline = first_baseline;
    let mut previous_after = 0.0;
    for (index, line) in lines.iter_mut().enumerate() {
        if index > 0 {
            baseline += line_height;
            if line.paragraph_first {
                baseline += previous_after + line.paragraph_spacing_before;
            }
        }
        line.baseline.y = baseline;
        previous_after = if line.paragraph_last {
            line.paragraph_spacing_after
        } else {
            0.0
        };
    }
    *content_height = lines
        .last()
        .map_or(0.0, |line| line.baseline.y + final_descent);
}

pub(super) fn layout_width(lines: &[HorizontalLine]) -> f32 {
    lines
        .iter()
        .map(|positioned| positioned.line.advance().x.abs() + positioned.inline_insets)
        .fold(0.0, f32::max)
}

pub(super) fn layout_height(lines: &[HorizontalLine], default_style: &TextStyle) -> f32 {
    let fallback = default_style
        .fonts()
        .first()
        .map(|font| font_line_metrics(&font.resolve(), default_style.font_size()));
    lines.last().map_or(0.0, |positioned| {
        let metrics = text_line_metrics(&positioned.line)
            .or(fallback)
            .unwrap_or_default();
        positioned.baseline.y + metrics.descent
    })
}

pub(super) fn text_line_metrics(line: &TextLine) -> Option<LineMetrics> {
    line.runs
        .iter()
        .map(|positioned| {
            let metrics = font_line_metrics(positioned.run.font(), positioned.run.font_size());
            let offset = positioned.placement.offset().y;
            LineMetrics {
                ascent: (metrics.ascent - offset).max(0.0),
                descent: (metrics.descent + offset).max(0.0),
                leading: metrics.leading,
            }
        })
        .reduce(|left, right| LineMetrics {
            ascent: left.ascent.max(right.ascent),
            descent: left.descent.max(right.descent),
            leading: left.leading.max(right.leading),
        })
}

pub(super) fn ellipsize_line(
    line: &mut HorizontalLine,
    attributed: &AttributedText,
    options: &ShapingOptions,
    tab_stops: &ResolvedTabStops,
) -> Result<(), TextError> {
    let paragraph_text = &attributed.content[line.paragraph_range.clone()];
    let bidi = bidi_info(paragraph_text, paragraph_level(options));
    let visible = &attributed.content[line.source_range.clone()];
    let mut boundaries: Vec<_> = grapheme_indices(visible)
        .into_iter()
        .map(|(offset, _)| line.source_range.start + offset)
        .collect();
    boundaries.push(line.source_range.end);
    boundaries.sort_unstable();
    boundaries.dedup();
    for boundary in boundaries
        .into_iter()
        .rev()
        .filter(|boundary| *boundary > line.source_range.start)
    {
        let mut candidate = shape_bidi_line(
            attributed,
            line.paragraph_range.clone(),
            line.source_range.start - line.paragraph_range.start
                ..boundary - line.paragraph_range.start,
            &bidi,
            options,
            tab_stops,
        )?;
        let Some(run_index) = candidate
            .runs
            .iter()
            .position(|positioned| positioned.run.source_range.end == boundary)
        else {
            continue;
        };
        let Some(run) = candidate.runs[run_index]
            .run
            .with_appended_ellipsis(&attributed.content)
        else {
            continue;
        };
        Arc::make_mut(&mut candidate.runs)[run_index]
            .run
            .replace_shaped(run);
        candidate.reflow_horizontal_runs();
        if !line.line_box_width.is_finite() || candidate.advance().x.abs() <= line.line_box_width {
            line.line = candidate;
            line.source_range.end = boundary;
            return Ok(());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn horizontal_line(width: f32, paragraph_is_rtl: bool) -> HorizontalLine {
        HorizontalLine {
            line: TextLine {
                runs: Arc::new([]),
                advance: Offset::new(width, 0.0),
            },
            baseline: Offset::ZERO,
            source_range: 0..0,
            paragraph_is_rtl,
            justify_allowed: false,
            line_box_start: 0.0,
            line_box_width: 100.0,
            paragraph_index: 0,
            paragraph_first: true,
            paragraph_last: true,
            paragraph_spacing_before: 0.0,
            paragraph_spacing_after: 0.0,
            paragraph_range: 0..0,
            inline_insets: 0.0,
            alignment: TextAlignment::Start,
        }
    }

    #[test]
    fn line_alignment_respects_each_paragraphs_base_direction() {
        let mut lines = [horizontal_line(60.0, false), horizontal_line(40.0, true)];

        lines[0].alignment = TextAlignment::Start;
        lines[1].alignment = TextAlignment::Start;
        align_lines(&mut lines, 100.0);
        assert_eq!([lines[0].baseline.x, lines[1].baseline.x], [0.0, 60.0]);
        lines[0].alignment = TextAlignment::End;
        lines[1].alignment = TextAlignment::End;
        align_lines(&mut lines, 100.0);
        assert_eq!([lines[0].baseline.x, lines[1].baseline.x], [40.0, 0.0]);
        lines[0].alignment = TextAlignment::Center;
        lines[1].alignment = TextAlignment::Center;
        align_lines(&mut lines, 100.0);
        assert_eq!([lines[0].baseline.x, lines[1].baseline.x], [20.0, 30.0]);
        lines[0].alignment = TextAlignment::Left;
        lines[1].alignment = TextAlignment::Left;
        align_lines(&mut lines, 100.0);
        assert_eq!([lines[0].baseline.x, lines[1].baseline.x], [0.0, 0.0]);
        lines[0].alignment = TextAlignment::Right;
        lines[1].alignment = TextAlignment::Right;
        align_lines(&mut lines, 100.0);
        assert_eq!([lines[0].baseline.x, lines[1].baseline.x], [40.0, 60.0]);
    }

    #[test]
    fn overfull_lines_overflow_in_the_direction_implied_by_alignment() {
        let mut lines = [horizontal_line(140.0, false), horizontal_line(140.0, true)];

        lines
            .iter_mut()
            .for_each(|line| line.alignment = TextAlignment::Start);
        align_lines(&mut lines, 100.0);
        assert_eq!([lines[0].baseline.x, lines[1].baseline.x], [0.0, -40.0]);
        lines
            .iter_mut()
            .for_each(|line| line.alignment = TextAlignment::End);
        align_lines(&mut lines, 100.0);
        assert_eq!([lines[0].baseline.x, lines[1].baseline.x], [-40.0, 0.0]);
        lines
            .iter_mut()
            .for_each(|line| line.alignment = TextAlignment::Center);
        align_lines(&mut lines, 100.0);
        assert_eq!([lines[0].baseline.x, lines[1].baseline.x], [-20.0, -20.0]);
    }

    #[test]
    fn fixed_line_height_controls_baseline_distance_without_clipping() {
        let mut lines = vec![
            horizontal_line(0.0, false),
            horizontal_line(0.0, false),
            horizontal_line(0.0, false),
        ];
        lines[0].baseline.y = 10.0;
        lines[1].baseline.y = 20.0;
        lines[2].baseline.y = 30.0;
        let mut content_height = 35.0;

        apply_fixed_line_height(&mut lines, &mut content_height, 12.0);
        assert_eq!(lines[0].baseline.y, 10.0);
        assert_eq!(lines[1].baseline.y, 22.0);
        assert_eq!(lines[2].baseline.y, 34.0);
        assert_eq!(content_height, 39.0);
    }
}
