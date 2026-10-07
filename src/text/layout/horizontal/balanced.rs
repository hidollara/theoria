use std::ops::Range;

use unicode_bidi::ParagraphBidiInfo;

use super::*;

#[derive(Clone)]
struct State {
    cost: f64,
    previous_node: usize,
    previous_hyphens: usize,
    line: TextLine,
}

pub(super) fn wrap(
    context: &WrapContext<'_>,
    paragraph: Range<usize>,
    paragraph_index: usize,
    paragraph_is_rtl: bool,
    bidi: &ParagraphBidiInfo<'_>,
    candidates: &[super::super::line_breaking::BreakCandidate],
) -> Result<Vec<HorizontalLine>, TextError> {
    let WrapContext {
        attributed,
        max_width,
        options,
        tab_stops,
        hyphenation_options,
        paragraph_style,
        hanging_punctuation,
        ..
    } = *context;
    let paragraph_text = &attributed.content[paragraph.clone()];
    let maximum_hyphens = hyphenation_options
        .maximum_consecutive_lines()
        .unwrap_or(candidates.len());
    let state_width = maximum_hyphens.saturating_add(1).max(1);
    let mut states: Vec<Vec<Option<State>>> = vec![vec![None; state_width]; candidates.len() + 1];
    states[0][0] = Some(State {
        cost: 0.0,
        previous_node: 0,
        previous_hyphens: 0,
        line: TextLine::default(),
    });

    for node in 0..candidates.len() {
        if states[node].iter().all(Option::is_none) {
            continue;
        }
        let line_start = if node == 0 {
            0
        } else {
            candidates[node - 1].index
        };
        let paragraph_first = line_start == 0;
        let logical_start_indent = paragraph_style.start_indent()
            + if paragraph_first {
                paragraph_style.first_line_indent()
            } else {
                0.0
            };
        let line_width = (max_width - logical_start_indent - paragraph_style.end_indent()).max(0.0);

        let mut edges = Vec::new();
        for (offset, candidate) in candidates[node..].iter().enumerate() {
            let target = node + offset + 1;
            let mut shaped = shape_bidi_line(
                attributed,
                paragraph.clone(),
                line_start..candidate.index,
                bidi,
                options,
                tab_stops,
            )?;
            if candidate.show_hyphen
                && candidate.index < paragraph_text.len()
                && !super::super::line_breaking::realize_horizontal_hyphen(
                    &mut shaped,
                    &attributed.content,
                    paragraph.start + candidate.index,
                )
            {
                continue;
            }
            let allowance = if hanging_punctuation == HangingPunctuation::End {
                hanging_end_allowance(
                    &shaped,
                    &attributed.content,
                    paragraph.start + candidate.index,
                    WritingMode::Horizontal,
                )
            } else {
                0.0
            };
            edges.push((
                target,
                candidate,
                shaped.clone(),
                shaped.advance().x.abs() - allowance,
            ));
        }
        let has_ordinary_fit = edges
            .iter()
            .any(|(_, candidate, _, advance)| !candidate.emergency && *advance <= line_width);

        for consecutive in 0..state_width {
            let Some(current) = states[node][consecutive].as_ref().cloned() else {
                continue;
            };
            for (target, candidate, shaped, advance) in &edges {
                if candidate.emergency && has_ordinary_fit && *advance <= line_width {
                    continue;
                }
                if candidate.automatic_hyphen && consecutive >= maximum_hyphens {
                    continue;
                }
                let next_hyphens =
                    if candidate.show_hyphen && candidate.index < paragraph_text.len() {
                        consecutive.saturating_add(1)
                    } else {
                        0
                    };
                if next_hyphens >= state_width {
                    continue;
                }
                let final_line = candidate.index == paragraph_text.len();
                let remaining = f64::from(line_width - *advance);
                let mut edge_cost = if remaining >= 0.0 {
                    if final_line {
                        0.0
                    } else {
                        remaining * remaining
                    }
                } else {
                    1_000_000.0 + remaining * remaining * 100.0
                };
                if candidate.show_hyphen && !final_line {
                    edge_cost += 400.0;
                }
                if candidate.emergency {
                    edge_cost += 100_000.0;
                }
                let cost = current.cost + edge_cost;
                let destination = &mut states[*target][next_hyphens];
                if destination.as_ref().is_none_or(|state| cost < state.cost) {
                    *destination = Some(State {
                        cost,
                        previous_node: node,
                        previous_hyphens: consecutive,
                        line: shaped.clone(),
                    });
                }
            }
        }
    }

    reconstruct(
        states,
        candidates,
        paragraph,
        paragraph_text,
        context,
        paragraph_index,
        paragraph_is_rtl,
    )
}

fn reconstruct(
    states: Vec<Vec<Option<State>>>,
    candidates: &[super::super::line_breaking::BreakCandidate],
    paragraph: Range<usize>,
    paragraph_text: &str,
    context: &WrapContext<'_>,
    paragraph_index: usize,
    paragraph_is_rtl: bool,
) -> Result<Vec<HorizontalLine>, TextError> {
    let max_width = context.max_width;
    let paragraph_style = context.paragraph_style;
    let final_node = candidates.len();
    let Some((mut consecutive, _)) = states[final_node]
        .iter()
        .enumerate()
        .filter_map(|(index, state)| state.as_ref().map(|state| (index, state.cost)))
        .min_by(|left, right| left.1.total_cmp(&right.1))
    else {
        unreachable!("the mandatory paragraph-end candidate always supplies a path");
    };
    let mut node = final_node;
    let mut reversed = Vec::new();
    while node > 0 {
        let state = states[node][consecutive]
            .as_ref()
            .expect("balanced predecessor remains available");
        let line_start = if state.previous_node == 0 {
            0
        } else {
            candidates[state.previous_node - 1].index
        };
        let line_end = candidates[node - 1].index;
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
        reversed.push(HorizontalLine {
            line: state.line.clone(),
            baseline: Offset::ZERO,
            source_range: paragraph.start + line_start..paragraph.start + line_end,
            paragraph_is_rtl,
            justify_allowed: line_end < paragraph_text.len(),
            line_box_start: physical_left,
            line_box_width: (max_width - logical_start_indent - paragraph_style.end_indent())
                .max(0.0),
            paragraph_index,
            paragraph_first,
            paragraph_last: line_end == paragraph_text.len(),
            paragraph_spacing_before: paragraph_style.spacing_before(),
            paragraph_spacing_after: paragraph_style.spacing_after(),
            paragraph_range: paragraph.clone(),
            inline_insets: logical_start_indent + paragraph_style.end_indent(),
            alignment: context.alignment,
        });
        node = state.previous_node;
        consecutive = state.previous_hyphens;
    }
    reversed.reverse();
    Ok(reversed)
}
