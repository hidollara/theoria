use std::{ops::Range, sync::Arc};

use unicode_bidi::{Level, ParagraphBidiInfo};

use crate::{
    font::{BaselineAxis, BaselineKind, ResolvedFont},
    geometry::Offset,
    transform::Affine2,
};

use super::shaping::{
    GlyphRun, ShapingDirection, ShapingError, shape_directional_segment,
    shape_directional_segment_in_context,
};
use super::unicode::{
    VerticalGlyphOrientation, bidi_info, grapheme_indices, vertical_glyph_orientation,
};
use super::{
    HorizontalDirection, ShapingOptions, Span, TextError, TextPaint, TextStyle, VerticalOrientation,
};

#[cfg(test)]
use crate::font::Font;

mod attributed_text;
mod horizontal;
mod justification;
mod line;
mod line_breaking;
mod metrics;
mod options;
mod output;
mod run;
mod spacing;
mod vertical;

use attributed_text::{AttributedText, StyleRange};
pub use horizontal::{HorizontalText, TextAlignment};
pub(in crate::text) use line::TextLine;
pub use line_breaking::{
    HyphenationMark, HyphenationOpportunity, HyphenationOptions, Hyphenator, LineBreakStrategy,
    LineBreakStyle, OverflowWrap,
};
use metrics::*;
pub use options::{
    HangingPunctuation, JustificationOptions, ParagraphStyle, TabAlignment, TabStop, TabStopError,
    TabStops, TextOverflow,
};
pub use output::{
    InlineAlignment, Text, TextBounds, TextLineInfo, TextVerticalAlignment, WritingMode,
};
pub(super) use output::{PaintRange, PositionedTextLine};
pub(in crate::text) use run::{LayoutRun, PositionedRun, RunOrientation};
use spacing::{InlineRun, ResolvedTabStops};
pub use vertical::{VerticalColumnDirection, VerticalText};

fn validate_font_size(font_size: f32) -> Result<(), TextError> {
    if !font_size.is_finite() {
        return Err(TextError::NonFiniteFontSize);
    }
    if font_size < 0.0 {
        return Err(TextError::NegativeFontSize);
    }
    Ok(())
}

fn shape_spaced_segment(
    style: &TextStyle,
    text: &str,
    options: &ShapingOptions,
    source_offset: usize,
    content: &str,
    spacing_group: usize,
    tab_stops: &ResolvedTabStops,
) -> Result<Vec<InlineRun>, ShapingError> {
    let mut groups = Vec::new();
    let mut text_start = 0;
    for (tab_start, _) in text.match_indices('\t') {
        if text_start < tab_start {
            groups.push(shape_text_group(
                style,
                &text[text_start..tab_start],
                options,
                source_offset + text_start,
                content,
                spacing_group,
            )?);
        }
        let tab_run =
            shape_directional_segment(style.fonts(), " ", options, source_offset + tab_start)?
                .into_iter()
                .map(|run| InlineRun::tab(run, style.font_size(), tab_stops.clone()))
                .collect();
        groups.push(tab_run);
        text_start = tab_start + 1;
    }
    if text_start < text.len() {
        groups.push(shape_text_group(
            style,
            &text[text_start..],
            options,
            source_offset + text_start,
            content,
            spacing_group,
        )?);
    }
    if options.direction.is_some_and(ShapingDirection::is_backward) {
        groups.reverse();
    }
    Ok(groups.into_iter().flatten().collect())
}

fn shape_text_group(
    style: &TextStyle,
    text: &str,
    options: &ShapingOptions,
    source_offset: usize,
    content: &str,
    spacing_group: usize,
) -> Result<Vec<InlineRun>, ShapingError> {
    Ok(
        shape_directional_segment_in_context(style.fonts(), text, options, source_offset, content)?
            .into_iter()
            .map(|run| {
                InlineRun::new(
                    run,
                    style.font_size(),
                    spacing_group,
                    style.letter_spacing(),
                    style.word_spacing(),
                    content,
                    style.baseline_shift(),
                )
            })
            .collect(),
    )
}

fn resolve_tab_interval(
    style: &TextStyle,
    context: &ShapingOptions,
    tab_size: f32,
) -> Result<f32, ShapingError> {
    if tab_size == 0.0 {
        return Ok(0.0);
    }
    let options = style
        .shaping_options(context)
        .direction(HorizontalDirection::LeftToRight);
    let space_advance: f32 = shape_directional_segment(style.fonts(), " ", &options, 0)?
        .into_iter()
        .map(|run| LayoutRun::new(run, style.font_size()).advance().x.abs())
        .sum();
    Ok(((space_advance + style.letter_spacing() + style.word_spacing()) * tab_size).max(0.0))
}

fn baseline_coordinate(run: &LayoutRun, axis: BaselineAxis, baseline: BaselineKind) -> Option<f32> {
    let coordinate = run
        .font()
        .baseline_coordinate(axis, run.script(), baseline)?;
    Some(coordinate * run.font_size() / f32::from(run.font().metrics().units_per_em()))
}

fn hanging_end_allowance(
    line: &TextLine,
    content: &str,
    break_index: usize,
    writing_mode: WritingMode,
) -> f32 {
    let Some((cluster, character)) = content
        .get(..break_index)
        .into_iter()
        .flat_map(str::char_indices)
        .rev()
        .find(|(_, character)| !character.is_whitespace())
    else {
        return 0.0;
    };
    if !matches!(
        character,
        '\u{3001}' | '\u{3002}' | '\u{ff0c}' | '\u{ff0e}' | ',' | '.'
    ) {
        return 0.0;
    }
    line.runs
        .iter()
        .filter(|positioned| positioned.run.source_range.contains(&cluster))
        .flat_map(|positioned| {
            let scale =
                positioned.run.font_size / f32::from(positioned.run.font.metrics().units_per_em());
            positioned
                .run
                .glyphs
                .iter()
                .filter(move |glyph| glyph.cluster == cluster)
                .map(move |glyph| match (writing_mode, positioned.orientation) {
                    (WritingMode::Horizontal, _) => glyph.advance.x.abs() * scale,
                    (WritingMode::Vertical, RunOrientation::Upright) => {
                        glyph.advance.y.abs() * scale
                    }
                    (WritingMode::Vertical, _) => {
                        glyph.advance.x.abs() * scale * positioned.inline_scale
                    }
                })
        })
        .sum()
}
fn paragraph_level(options: &ShapingOptions) -> Option<Level> {
    match options.direction {
        Some(ShapingDirection::LeftToRight) => Some(Level::ltr()),
        Some(ShapingDirection::RightToLeft) => Some(Level::rtl()),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
