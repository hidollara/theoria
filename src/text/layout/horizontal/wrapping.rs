use super::*;

pub(super) fn wrap_paragraph(
    context: &WrapContext<'_>,
    paragraph: Range<usize>,
    paragraph_index: usize,
) -> Result<Vec<HorizontalLine>, TextError> {
    let WrapContext {
        attributed,
        max_width,
        options,
        tab_stops,
        hyphenator,
        hyphenation_options,
        paragraph_style,
        line_break_style,
        overflow_wrap,
        strategy,
        hanging_punctuation,
        alignment,
    } = *context;
    let paragraph_text = &attributed.content[paragraph.clone()];
    if paragraph_text.is_empty() {
        let paragraph_is_rtl = paragraph_level(options).is_some_and(|level| level.is_rtl());
        return Ok(vec![HorizontalLine {
            line: TextLine::default(),
            baseline: Vector2::ZERO,
            source_range: paragraph.start..paragraph.start,
            paragraph_is_rtl,
            justify_allowed: false,
            line_box_start: paragraph_style.start_indent(),
            line_box_width: (max_width
                - paragraph_style.start_indent()
                - paragraph_style.end_indent())
            .max(0.0),
            paragraph_index,
            paragraph_first: true,
            paragraph_last: true,
            paragraph_spacing_before: paragraph_style.spacing_before(),
            paragraph_spacing_after: paragraph_style.spacing_after(),
            paragraph_range: paragraph.clone(),
            inline_insets: paragraph_style.start_indent()
                + paragraph_style.first_line_indent()
                + paragraph_style.end_indent(),
            alignment,
        }]);
    }

    let bidi = bidi_info(paragraph_text, paragraph_level(options));
    let paragraph_is_rtl = bidi.paragraph_level.is_rtl();
    let automatic = hyphenator.map_or_else(Vec::new, |hyphenator| {
        attributed.hyphenation_boundaries(
            paragraph.clone(),
            options,
            hyphenator,
            hyphenation_options,
        )
    });
    let candidates = super::super::line_breaking::candidates(
        paragraph_text,
        &automatic,
        line_break_style,
        options
            .language
            .as_deref()
            .or(attributed.default_style.language()),
        overflow_wrap,
    );
    if strategy == LineBreakStrategy::Balanced && max_width.is_finite() {
        return balanced::wrap(
            context,
            paragraph,
            paragraph_index,
            paragraph_is_rtl,
            &bidi,
            &candidates,
        );
    }
    let mut lines = Vec::new();
    let mut line_start = 0;
    let mut consecutive_hyphenated_lines = 0usize;
    while line_start < paragraph_text.len() {
        let paragraph_first = line_start == 0;
        let logical_start_indent = paragraph_style.start_indent()
            + if paragraph_first {
                paragraph_style.first_line_indent()
            } else {
                0.0
            };
        let physical_left = if paragraph_is_rtl {
            paragraph_style.end_indent()
        } else {
            logical_start_indent
        };
        let line_box_width = if max_width.is_finite() {
            (max_width - logical_start_indent - paragraph_style.end_indent()).max(0.0)
        } else {
            f32::INFINITY
        };
        let mut best_fit = None;
        let mut emergency_fit = None;
        let mut chosen = None;
        for candidate in candidates
            .iter()
            .filter(|candidate| candidate.index > line_start)
        {
            if candidate.automatic_hyphen
                && hyphenation_options
                    .maximum_consecutive_lines()
                    .is_some_and(|maximum| consecutive_hyphenated_lines >= maximum)
            {
                continue;
            }
            let line_end = candidate.index;
            let opportunity = candidate.opportunity;
            let mut line = shape_bidi_line(
                attributed,
                paragraph.clone(),
                line_start..line_end,
                &bidi,
                options,
                tab_stops,
            )?;
            if candidate.show_hyphen
                && line_end < paragraph_text.len()
                && !super::super::line_breaking::realize_horizontal_hyphen(
                    &mut line,
                    &attributed.content,
                    paragraph.start + line_end,
                )
            {
                continue;
            }
            let allowance = if hanging_punctuation == HangingPunctuation::End {
                hanging_end_allowance(
                    &line,
                    &attributed.content,
                    paragraph.start + line_end,
                    WritingMode::Horizontal,
                )
            } else {
                0.0
            };
            let fits = line.advance().x.abs() - allowance <= line_box_width;
            match opportunity {
                BreakOpportunity::Allowed if fits => {
                    if candidate.emergency {
                        if best_fit.is_none() {
                            emergency_fit = Some((line_end, line));
                        }
                    } else {
                        best_fit = Some((line_end, line));
                    }
                }
                BreakOpportunity::Allowed => {
                    chosen = Some(
                        best_fit
                            .take()
                            .or_else(|| emergency_fit.take())
                            .unwrap_or((line_end, line)),
                    );
                    break;
                }
                BreakOpportunity::Mandatory if fits => {
                    chosen = Some((line_end, line));
                    break;
                }
                BreakOpportunity::Mandatory => {
                    chosen = Some(
                        best_fit
                            .take()
                            .or_else(|| emergency_fit.take())
                            .unwrap_or((line_end, line)),
                    );
                    break;
                }
            }
        }

        let (line_end, line) = chosen.expect("Unicode line breaking always includes end-of-text");
        let used_hyphen = candidates.iter().any(|candidate| {
            candidate.index == line_end && candidate.show_hyphen && line_end < paragraph_text.len()
        });
        consecutive_hyphenated_lines = if used_hyphen {
            consecutive_hyphenated_lines + 1
        } else {
            0
        };
        lines.push(HorizontalLine {
            line,
            baseline: Vector2::ZERO,
            source_range: paragraph.start + line_start..paragraph.start + line_end,
            paragraph_is_rtl,
            justify_allowed: line_end < paragraph_text.len(),
            line_box_start: physical_left,
            line_box_width,
            paragraph_index,
            paragraph_first,
            paragraph_last: line_end == paragraph_text.len(),
            paragraph_spacing_before: paragraph_style.spacing_before(),
            paragraph_spacing_after: paragraph_style.spacing_after(),
            paragraph_range: paragraph.clone(),
            inline_insets: logical_start_indent + paragraph_style.end_indent(),
            alignment,
        });
        line_start = line_end;
    }
    Ok(lines)
}
