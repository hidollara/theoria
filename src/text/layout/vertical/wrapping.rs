use super::*;

pub(super) fn wrap_vertical_paragraph(
    context: &VerticalWrapContext<'_>,
    paragraph: Range<usize>,
) -> Result<Vec<VerticalColumn>, TextError> {
    let VerticalWrapContext {
        attributed,
        max_height,
        options,
        tab_stops,
        hyphenator,
        hyphenation_options,
        line_break_style,
        overflow_wrap,
        hanging_punctuation,
        paragraph_style,
        alignment,
    } = *context;
    let paragraph_text = &attributed.content[paragraph.clone()];
    if paragraph_text.is_empty() {
        return Ok(vec![VerticalColumn {
            line: TextLine::default(),
            baseline: Vector2::ZERO,
            source_range: paragraph.start..paragraph.start,
            justify_allowed: false,
            line_box_start: paragraph_style.start_indent() + paragraph_style.first_line_indent(),
            line_box_height: (max_height
                - paragraph_style.start_indent()
                - paragraph_style.first_line_indent()
                - paragraph_style.end_indent())
            .max(0.0),
            paragraph_first: true,
            paragraph_last: true,
            paragraph_spacing_before: paragraph_style.spacing_before(),
            paragraph_spacing_after: paragraph_style.spacing_after(),
            inline_insets: paragraph_style.start_indent()
                + paragraph_style.first_line_indent()
                + paragraph_style.end_indent(),
            alignment,
        }]);
    }

    let automatic = hyphenator.map_or_else(Vec::new, |hyphenator| {
        attributed.hyphenation_boundaries(
            paragraph.clone(),
            options,
            hyphenator,
            hyphenation_options,
        )
    });
    let candidates = super::line_breaking::candidates(
        paragraph_text,
        &automatic,
        line_break_style,
        options
            .language
            .as_deref()
            .or(attributed.default_style.language()),
        overflow_wrap,
    );
    let mut columns = Vec::new();
    let mut column_start = 0;
    let mut consecutive_hyphenated_columns = 0usize;
    while column_start < paragraph_text.len() {
        let paragraph_first = column_start == 0;
        let logical_start_indent = paragraph_style.start_indent()
            + if paragraph_first {
                paragraph_style.first_line_indent()
            } else {
                0.0
            };
        let line_box_height = if max_height.is_finite() {
            (max_height - logical_start_indent - paragraph_style.end_indent()).max(0.0)
        } else {
            f32::INFINITY
        };
        let mut best_fit = None;
        let mut emergency_fit = None;
        let mut chosen = None;
        for candidate in candidates
            .iter()
            .filter(|candidate| candidate.index > column_start)
        {
            if candidate.automatic_hyphen
                && hyphenation_options
                    .maximum_consecutive_lines()
                    .is_some_and(|maximum| consecutive_hyphenated_columns >= maximum)
            {
                continue;
            }
            let column_end = candidate.index;
            let opportunity = candidate.opportunity;
            let mut column = shape_vertical_range(
                attributed,
                paragraph.start + column_start..paragraph.start + column_end,
                options,
                tab_stops,
            )?;
            if candidate.show_hyphen
                && column_end < paragraph_text.len()
                && !super::line_breaking::realize_vertical_hyphen(
                    &mut column,
                    &attributed.content,
                    paragraph.start + column_end,
                )
            {
                continue;
            }
            let allowance = if hanging_punctuation == HangingPunctuation::End {
                hanging_end_allowance(
                    &column,
                    &attributed.content,
                    paragraph.start + column_end,
                    WritingMode::Vertical,
                )
            } else {
                0.0
            };
            let fits = column.advance().y.abs() - allowance <= line_box_height;
            match opportunity {
                BreakOpportunity::Allowed if fits => {
                    if candidate.emergency {
                        if best_fit.is_none() {
                            emergency_fit = Some((column_end, column));
                        }
                    } else {
                        best_fit = Some((column_end, column));
                    }
                }
                BreakOpportunity::Allowed => {
                    chosen = Some(
                        best_fit
                            .take()
                            .or_else(|| emergency_fit.take())
                            .unwrap_or((column_end, column)),
                    );
                    break;
                }
                BreakOpportunity::Mandatory if fits => {
                    chosen = Some((column_end, column));
                    break;
                }
                BreakOpportunity::Mandatory => {
                    chosen = Some(
                        best_fit
                            .take()
                            .or_else(|| emergency_fit.take())
                            .unwrap_or((column_end, column)),
                    );
                    break;
                }
            }
        }

        let (column_end, line) = chosen.expect("Unicode line breaking always includes end-of-text");
        let used_hyphen = candidates.iter().any(|candidate| {
            candidate.index == column_end
                && candidate.show_hyphen
                && column_end < paragraph_text.len()
        });
        consecutive_hyphenated_columns = if used_hyphen {
            consecutive_hyphenated_columns + 1
        } else {
            0
        };
        columns.push(VerticalColumn {
            line,
            baseline: Vector2::ZERO,
            source_range: paragraph.start + column_start..paragraph.start + column_end,
            justify_allowed: column_end < paragraph_text.len(),
            line_box_start: logical_start_indent,
            line_box_height,
            paragraph_first,
            paragraph_last: column_end == paragraph_text.len(),
            paragraph_spacing_before: paragraph_style.spacing_before(),
            paragraph_spacing_after: paragraph_style.spacing_after(),
            inline_insets: logical_start_indent + paragraph_style.end_indent(),
            alignment,
        });
        column_start = column_end;
    }
    Ok(columns)
}

pub(super) fn ellipsize_column(
    column: &mut VerticalColumn,
    attributed: &AttributedText,
    options: &ShapingOptions,
    tab_stops: &ResolvedTabStops,
    maximum_height: f32,
) -> Result<(), TextError> {
    let visible = &attributed.content[column.source_range.clone()];
    let mut boundaries: Vec<_> = grapheme_indices(visible)
        .into_iter()
        .map(|(offset, _)| column.source_range.start + offset)
        .collect();
    boundaries.push(column.source_range.end);
    boundaries.sort_unstable();
    boundaries.dedup();
    for boundary in boundaries
        .into_iter()
        .rev()
        .filter(|boundary| *boundary > column.source_range.start)
    {
        let mut candidate = shape_vertical_range(
            attributed,
            column.source_range.start..boundary,
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
        if !candidate.replace_vertical_run(run_index, run) {
            continue;
        }
        if !maximum_height.is_finite() || candidate.advance().y.abs() <= maximum_height {
            column.line = candidate;
            column.source_range.end = boundary;
            return Ok(());
        }
    }
    Ok(())
}
