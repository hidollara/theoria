use std::sync::Arc;

use crate::{geometry::Offset, transform::Affine2};

use super::{JustificationOptions, PositionedRun, RunOrientation, TextLine, spacing};

mod arabic;
mod cjk;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Opportunity {
    WordSeparator,
    CjkInterCharacter,
}

impl TextLine {
    pub(super) fn justify_to_width(
        &mut self,
        content: &str,
        width: f32,
        options: JustificationOptions,
    ) {
        let mut remaining = width - self.advance.x.abs();
        if remaining <= 0.0 {
            return;
        }
        arabic::insert_to_width(self, content, width, options.maximum_tatweels_per_line());
        self.reflow_horizontal_runs();
        remaining = width - self.advance.x.abs();
        if remaining <= 0.0 {
            return;
        }
        let Some((opportunity, opportunity_count)) = self.opportunity(content, true) else {
            return;
        };
        let adjustment = options
            .maximum_spacing_expansion()
            .map_or(remaining / opportunity_count as f32, |maximum| {
                (remaining / opportunity_count as f32).min(maximum.max(0.0))
            });
        self.apply_spacing(content, opportunity, adjustment, true);
        self.reflow_horizontal_runs();
    }

    pub(super) fn justify_to_height(
        &mut self,
        content: &str,
        height: f32,
        options: JustificationOptions,
    ) {
        let remaining = height - self.advance.y.abs();
        if remaining <= 0.0 {
            return;
        }
        let Some((opportunity, opportunity_count)) = self.opportunity(content, false) else {
            return;
        };
        let adjustment = options
            .maximum_spacing_expansion()
            .map_or(remaining / opportunity_count as f32, |maximum| {
                (remaining / opportunity_count as f32).min(maximum.max(0.0))
            });
        let advances: Vec<_> = self.runs.iter().map(vertical_run_advance).collect();
        self.apply_spacing(content, opportunity, adjustment, false);
        let mut shift = 0.0;
        for (positioned, before) in Arc::make_mut(&mut self.runs).iter_mut().zip(advances) {
            positioned.placement = positioned
                .placement
                .then(
                    Affine2::from_translation(Offset::new(0.0, shift)).expect("finite text offset"),
                )
                .expect("finite text placement");
            positioned.inline_start += shift;
            if positioned.orientation == RunOrientation::Combined {
                continue;
            }
            shift += vertical_run_advance(positioned) - before;
        }
        self.advance.y += shift;
    }

    fn opportunity(&self, content: &str, include_combined: bool) -> Option<(Opportunity, usize)> {
        for opportunity in [Opportunity::WordSeparator, Opportunity::CjkInterCharacter] {
            let mut count: usize = self
                .runs
                .iter()
                .filter(|positioned| participates(positioned, include_combined))
                .map(|positioned| match opportunity {
                    Opportunity::WordSeparator => {
                        spacing::word_separator_count(&positioned.run, content)
                    }
                    Opportunity::CjkInterCharacter => {
                        cjk::opportunity_count(&positioned.run, content)
                    }
                })
                .sum();
            if opportunity == Opportunity::CjkInterCharacter {
                count += self
                    .runs
                    .windows(2)
                    .filter(|runs| {
                        participates(&runs[0], include_combined)
                            && participates(&runs[1], include_combined)
                            && cjk::has_boundary(&runs[0].run, &runs[1].run, content)
                    })
                    .count();
            }
            if count > 0 {
                return Some((opportunity, count));
            }
        }
        None
    }

    fn apply_spacing(
        &mut self,
        content: &str,
        opportunity: Opportunity,
        adjustment: f32,
        include_combined: bool,
    ) {
        let cross_run_boundaries = if opportunity == Opportunity::CjkInterCharacter {
            self.runs
                .windows(2)
                .enumerate()
                .filter_map(|(index, runs)| {
                    (participates(&runs[0], include_combined)
                        && participates(&runs[1], include_combined)
                        && cjk::has_boundary(&runs[0].run, &runs[1].run, content))
                    .then_some(index)
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let runs = Arc::make_mut(&mut self.runs);
        for positioned in runs
            .iter_mut()
            .filter(|positioned| participates(positioned, include_combined))
        {
            match opportunity {
                Opportunity::WordSeparator => {
                    let font_size = positioned.run.font_size;
                    spacing::apply_word_spacing(
                        &mut positioned.run,
                        font_size,
                        content,
                        adjustment,
                    );
                }
                Opportunity::CjkInterCharacter => {
                    cjk::apply_spacing(&mut positioned.run, content, adjustment);
                }
            }
        }
        for index in cross_run_boundaries {
            let font_size = runs[index].run.font_size;
            spacing::add_spacing_to_last_glyph(&mut runs[index].run, font_size, adjustment);
        }
    }
}

fn participates(positioned: &PositionedRun, include_combined: bool) -> bool {
    include_combined || positioned.orientation != RunOrientation::Combined
}

fn vertical_run_advance(positioned: &PositionedRun) -> f32 {
    match positioned.orientation {
        RunOrientation::Upright => positioned.run.advance().y,
        RunOrientation::Sideways | RunOrientation::Combined => {
            positioned.run.advance().x.abs() * positioned.inline_scale
        }
    }
}
