use std::sync::Arc;

use crate::{math::Vector2, transform::Affine2};

use super::{GlyphRun, LayoutRun, PositionedRun, RunOrientation};

/// A sequence of already-shaped runs placed on one shared baseline.
#[derive(Clone, Debug)]
pub(in crate::text) struct TextLine {
    pub(in crate::text) runs: Arc<[PositionedRun]>,
    pub(in crate::text) advance: Vector2,
}

impl TextLine {
    pub(super) const fn advance(&self) -> Vector2 {
        self.advance
    }

    pub(super) fn reflow_horizontal_runs(&mut self) {
        let mut pen = 0.0;
        for positioned in Arc::make_mut(&mut self.runs) {
            let baseline_y = positioned.placement.translation().y;
            positioned.inline_start = pen;
            positioned.placement = Affine2::from_translation(Vector2::new(pen, baseline_y));
            pen += positioned.run.advance().x;
        }
        self.advance.x = pen;
    }

    pub(super) fn replace_vertical_run(&mut self, index: usize, shaped: GlyphRun) -> bool {
        let Some(positioned) = self.runs.get(index) else {
            return false;
        };
        let orientation = positioned.orientation;
        if orientation == RunOrientation::Combined {
            return false;
        }
        let inline_advance = |run: &LayoutRun| match orientation {
            RunOrientation::Upright => run.advance().y,
            RunOrientation::Sideways => run.advance().x.abs(),
            RunOrientation::Combined => unreachable!(),
        };
        let run = LayoutRun::new(shaped, positioned.run.font_size);
        let delta = inline_advance(&run) - inline_advance(&positioned.run);
        let positioned_runs = Arc::make_mut(&mut self.runs);
        positioned_runs[index].run = run;
        for positioned in &mut positioned_runs[index + 1..] {
            positioned.inline_start += delta;
            let placement = positioned.placement;
            positioned.placement = Affine2::new(
                placement.linear(),
                placement.translation() + Vector2::new(0.0, delta),
            );
        }
        self.advance.y += delta;
        true
    }
}

impl Default for TextLine {
    fn default() -> Self {
        Self {
            runs: Arc::from([]),
            advance: Vector2::ZERO,
        }
    }
}
