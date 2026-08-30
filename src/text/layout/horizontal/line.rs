use super::*;

impl TextLine {
    pub(in crate::text::layout) fn from_runs(
        runs: impl IntoIterator<Item = InlineRun>,
        content: &str,
    ) -> Self {
        let mut runs: Vec<_> = runs.into_iter().collect();
        for index in 0..runs.len().saturating_sub(1) {
            if runs[index].continues_with(&runs[index + 1]) {
                runs[index].add_trailing_letter_spacing();
            }
        }
        resolve_horizontal_tabs(&mut runs, content);
        let mut pen = Vector2::ZERO;
        let runs: Arc<[PositionedRun]> = runs
            .into_iter()
            .map(|run| {
                let baseline_shift = run.baseline_shift();
                let run = run.into_inner();
                let baseline_offset =
                    baseline_coordinate(&run, BaselineAxis::Horizontal, BaselineKind::Alphabetic)
                        .unwrap_or(0.0);
                let positioned = PositionedRun {
                    run,
                    placement: Affine2::from_translation(Vector2::new(
                        pen.x,
                        pen.y + baseline_offset - baseline_shift,
                    )),
                    inline_start: pen.x,
                    inline_scale: 1.0,
                    orientation: RunOrientation::Upright,
                };
                pen = pen + positioned.run.advance();
                positioned
            })
            .collect();
        Self { runs, advance: pen }
    }
}

fn resolve_horizontal_tabs(runs: &mut [InlineRun], content: &str) {
    let mut inline = 0.0;
    for index in 0..runs.len() {
        if runs[index].is_tab() {
            let field_end = runs[index + 1..]
                .iter()
                .position(InlineRun::is_tab)
                .map_or(runs.len(), |offset| index + 1 + offset);
            let field = &runs[index + 1..field_end];
            let field_width: f32 = field
                .iter()
                .map(InlineRun::natural_horizontal_advance)
                .sum();
            let decimal_anchor = runs[index].decimal_character(inline).and_then(|character| {
                let mut before = 0.0;
                for run in field {
                    if let Some(anchor) = run.decimal_anchor(content, character) {
                        return Some(before + anchor);
                    }
                    before += run.natural_horizontal_advance();
                }
                None
            });
            if let Some(advance) = runs[index].tab_advance(inline, field_width, decimal_anchor) {
                runs[index].set_horizontal_advance(advance);
            }
        }
        inline += runs[index].natural_horizontal_advance();
    }
}
