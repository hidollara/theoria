use super::*;

// `Oriented` is the common case. Boxing every run solely to make this short-lived
// construction enum smaller would add one allocation per run; `Combined` is the
// rare branch and the enum never survives layout.
#[allow(clippy::large_enum_variant)]
pub(super) enum VerticalItem {
    Oriented(InlineRun, RunOrientation),
    Combined { runs: Vec<InlineRun>, em_size: f32 },
}

impl TextLine {
    pub(super) fn from_vertical_items(
        items: impl IntoIterator<Item = VerticalItem>,
        content: &str,
    ) -> Self {
        let mut items: Vec<_> = items.into_iter().collect();
        resolve_vertical_tabs(&mut items, content);
        let mut inline = 0.0;
        let mut positioned_runs = Vec::new();
        let mut previous_spacing = None;
        for item in items {
            match item {
                VerticalItem::Oriented(spaced_run, orientation) => {
                    let baseline_shift = spaced_run.baseline_shift();
                    let next_spacing = spaced_run.spacing_key();
                    if previous_spacing
                        .zip(next_spacing)
                        .is_some_and(|((left, _), (right, _))| left == right)
                    {
                        inline += previous_spacing.map_or(0.0, |(_, spacing)| spacing);
                    }
                    let run = spaced_run.into_inner();
                    let advance = match orientation {
                        RunOrientation::Upright => run.advance().y,
                        RunOrientation::Sideways => run.advance().x.abs(),
                        RunOrientation::Combined => unreachable!("combined runs require a group"),
                    };
                    let orientation_transform = match orientation {
                        RunOrientation::Upright => Affine2::IDENTITY,
                        RunOrientation::Sideways => {
                            Affine2::from_rotation(std::f32::consts::FRAC_PI_2)
                                .expect("finite rotation")
                        }
                        RunOrientation::Combined => unreachable!("combined runs require a group"),
                    };
                    let cross = match orientation {
                        RunOrientation::Sideways => sideways_cross_offset(&run),
                        RunOrientation::Upright => 0.0,
                        RunOrientation::Combined => unreachable!("combined runs require a group"),
                    };
                    positioned_runs.push(PositionedRun {
                        run,
                        placement: orientation_transform
                            .then(
                                Affine2::from_translation(Offset::new(
                                    cross + baseline_shift,
                                    inline,
                                ))
                                .expect("finite translation"),
                            )
                            .expect("finite composed transform"),
                        inline_start: inline,
                        inline_scale: 1.0,
                        orientation,
                    });
                    inline += advance;
                    previous_spacing = next_spacing;
                }
                VerticalItem::Combined { runs, em_size } => {
                    position_combined_runs(&mut positioned_runs, runs, inline, em_size);
                    inline += em_size;
                    previous_spacing = None;
                }
            }
        }
        Self {
            runs: positioned_runs.into(),
            advance: Offset::new(0.0, inline),
        }
    }
}

fn resolve_vertical_tabs(items: &mut [VerticalItem], content: &str) {
    let mut inline = 0.0;
    for index in 0..items.len() {
        let is_tab = matches!(
            &items[index],
            VerticalItem::Oriented(run, _) if run.is_tab()
        );
        if is_tab {
            let field_end = items[index + 1..]
                .iter()
                .position(|item| matches!(item, VerticalItem::Oriented(run, _) if run.is_tab()))
                .map_or(items.len(), |offset| index + 1 + offset);
            let field_width: f32 = items[index + 1..field_end]
                .iter()
                .map(vertical_item_advance)
                .sum();
            let decimal_anchor = match &items[index] {
                VerticalItem::Oriented(run, _) => run.decimal_character(inline),
                VerticalItem::Combined { .. } => None,
            }
            .and_then(|character| {
                let mut before = 0.0;
                for item in &items[index + 1..field_end] {
                    if let VerticalItem::Oriented(run, RunOrientation::Sideways) = item
                        && let Some(anchor) = run.decimal_anchor(content, character)
                    {
                        return Some(before + anchor);
                    }
                    before += vertical_item_advance(item);
                }
                None
            });
            if let VerticalItem::Oriented(run, _) = &mut items[index]
                && let Some(advance) = run.tab_advance(inline, field_width, decimal_anchor)
            {
                run.set_horizontal_advance(advance);
            }
        }
        inline += vertical_item_advance(&items[index]);
    }
}

fn vertical_item_advance(item: &VerticalItem) -> f32 {
    match item {
        VerticalItem::Oriented(run, orientation) => match orientation {
            RunOrientation::Upright => run.natural_vertical_advance(),
            RunOrientation::Sideways => run.natural_horizontal_advance(),
            RunOrientation::Combined => unreachable!("combined runs require a group"),
        },
        VerticalItem::Combined { em_size, .. } => *em_size,
    }
}

fn sideways_cross_offset(run: &LayoutRun) -> f32 {
    let roman = baseline_coordinate(run, BaselineAxis::Vertical, BaselineKind::Alphabetic);
    let center = baseline_coordinate(run, BaselineAxis::Vertical, BaselineKind::IdeographicCenter);
    match (roman, center) {
        (Some(roman), Some(center)) => roman - center,
        _ => synthesized_sideways_cross_offset(run),
    }
}

pub(super) fn synthesized_sideways_cross_offset(run: &LayoutRun) -> f32 {
    let metrics = font_line_metrics(run.font(), run.font_size());
    (metrics.descent - metrics.ascent) * 0.5
}

fn position_combined_runs(
    output: &mut Vec<PositionedRun>,
    mut runs: Vec<InlineRun>,
    inline: f32,
    em_size: f32,
) {
    for index in 0..runs.len().saturating_sub(1) {
        if runs[index].continues_with(&runs[index + 1]) {
            runs[index].add_trailing_letter_spacing();
        }
    }
    let mut pen = 0.0_f32;
    let mut positions = Vec::with_capacity(runs.len());
    let mut minimum = 0.0_f32;
    let mut maximum = 0.0_f32;
    let mut ascent = 0.0_f32;
    let mut descent = 0.0_f32;
    for run in runs {
        let run = run.into_inner();
        let advance = run.advance().x;
        let end = pen + advance;
        minimum = minimum.min(pen.min(end));
        maximum = maximum.max(pen.max(end));
        let metrics = font_line_metrics(run.font(), run.font_size());
        let half_leading = metrics.leading * 0.5;
        ascent = ascent.max(metrics.ascent + half_leading);
        descent = descent.max(metrics.descent + half_leading);
        positions.push((run, pen, end));
        pen = end;
    }

    let width = maximum - minimum;
    let render_scale = if width > em_size && width > 0.0 {
        em_size / width
    } else {
        1.0
    };
    let inline_scale = if width > 0.0 { em_size / width } else { 1.0 };
    let center = (minimum + maximum) * 0.5;
    let baseline_y = inline + em_size * 0.5 + (ascent - descent) * render_scale * 0.5;

    for (run, start, end) in positions {
        let visual_start = start.min(end);
        output.push(PositionedRun {
            run,
            placement: Affine2::from_scale(render_scale, render_scale)
                .expect("finite scale")
                .then(
                    Affine2::from_translation(Offset::new(
                        (start - center) * render_scale,
                        baseline_y,
                    ))
                    .expect("finite translation"),
                )
                .expect("finite composed transform"),
            inline_start: inline + (visual_start - minimum) * inline_scale,
            inline_scale,
            orientation: RunOrientation::Combined,
        });
    }
}
