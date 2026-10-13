use std::ops::Range;

use crate::geometry::Point;

use super::super::layout::{RunOrientation, Text, WritingMode};

/// Maps a logical UTF-8 range to the physical regions occupied by that range.
///
/// This is layout geometry used while lowering text decorations. It carries no
/// caret, selection, hit-testing, or editor state.
pub(super) fn range_regions(text: &Text, source_range: Range<usize>) -> Vec<[Point; 4]> {
    if source_range.start > source_range.end
        || source_range.end > text.content.len()
        || !text.content.is_char_boundary(source_range.start)
        || !text.content.is_char_boundary(source_range.end)
        || source_range.is_empty()
    {
        return Vec::new();
    }

    let mut regions = Vec::new();
    for positioned_line in text.lines.iter() {
        let mut spans: Vec<_> = line_cells(positioned_line, text.writing_mode)
            .into_iter()
            .filter(|cell| {
                source_range.start < cell.source_range.end
                    && cell.source_range.start < source_range.end
            })
            .map(|cell| (cell.start, cell.end))
            .collect();
        spans.sort_by(|left, right| left.0.total_cmp(&right.0));

        let mut merged: Vec<(f32, f32)> = Vec::new();
        for (start, end) in spans {
            if let Some((_, previous_end)) = merged.last_mut()
                && start <= *previous_end
            {
                *previous_end = previous_end.max(end);
            } else {
                merged.push((start, end));
            }
        }

        let cross = match text.writing_mode {
            WritingMode::Horizontal => positioned_line.baseline.y,
            WritingMode::Vertical => positioned_line.baseline.x,
        };
        let cross_start = cross - positioned_line.metrics.ascent;
        let cross_end = cross + positioned_line.metrics.descent;
        regions.extend(
            merged
                .into_iter()
                .map(|(start, end)| match text.writing_mode {
                    WritingMode::Horizontal => [
                        Point::new(start, cross_start),
                        Point::new(end, cross_start),
                        Point::new(end, cross_end),
                        Point::new(start, cross_end),
                    ],
                    WritingMode::Vertical => [
                        Point::new(cross_start, start),
                        Point::new(cross_end, start),
                        Point::new(cross_end, end),
                        Point::new(cross_start, end),
                    ],
                }),
        );
    }
    regions
}

fn line_cells(
    positioned_line: &super::super::layout::PositionedTextLine,
    writing_mode: WritingMode,
) -> Vec<RangeCell> {
    let mut cells = Vec::new();
    for positioned_run in positioned_line.line.runs.iter() {
        let run = &positioned_run.run;
        let scale = run.font_size / f32::from(run.font.metrics().units_per_em());
        let mut boundaries: Vec<_> = run.glyphs.iter().map(|glyph| glyph.cluster).collect();
        boundaries.push(run.source_range.start);
        boundaries.push(run.source_range.end);
        boundaries.sort_unstable();
        boundaries.dedup();

        let mut inline = match writing_mode {
            WritingMode::Horizontal => positioned_line.baseline.x + positioned_run.inline_start,
            WritingMode::Vertical => positioned_line.baseline.y + positioned_run.inline_start,
        };
        let mut glyph_index = 0;
        while glyph_index < run.glyphs.len() {
            let cluster = run.glyphs[glyph_index].cluster;
            let start = inline;
            while glyph_index < run.glyphs.len() && run.glyphs[glyph_index].cluster == cluster {
                inline += match (writing_mode, positioned_run.orientation) {
                    (WritingMode::Horizontal, _) => run.glyphs[glyph_index].advance.x * scale,
                    (WritingMode::Vertical, RunOrientation::Upright) => {
                        -run.glyphs[glyph_index].advance.y * scale
                    }
                    (
                        WritingMode::Vertical,
                        RunOrientation::Sideways | RunOrientation::Combined,
                    ) => {
                        run.glyphs[glyph_index].advance.x.abs()
                            * scale
                            * positioned_run.inline_scale
                    }
                };
                glyph_index += 1;
            }
            let logical_end = boundaries
                .iter()
                .copied()
                .find(|boundary| *boundary > cluster)
                .unwrap_or(run.source_range.end);
            let (start, end) = (start.min(inline), start.max(inline));
            cells.push(RangeCell {
                source_range: cluster..logical_end,
                start,
                end,
            });
        }
    }
    cells
}

#[derive(Clone, Debug)]
struct RangeCell {
    source_range: Range<usize>,
    start: f32,
    end: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::{TextStyle, test_support::bundled_latin_font};

    #[test]
    fn a_logical_range_becomes_one_visual_region_per_wrapped_line() {
        let font = bundled_latin_font();
        let fonts = [font];
        let first_line_width = Text::horizontal(TextStyle::new(&fonts, 24.0))
            .push("abc ")
            .layout()
            .unwrap()
            .width();
        let text = Text::horizontal(TextStyle::new(&fonts, 24.0))
            .push("abc def")
            .wrap_width(first_line_width + 0.001)
            .layout()
            .unwrap();

        assert_eq!(text.line_count(), 2);
        assert_eq!(range_regions(&text, 0..text.content().len()).len(), 2);
    }
}
