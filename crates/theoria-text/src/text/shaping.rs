use crate::font::Font;

mod discretionary;
mod elongation;
mod error;
mod fallback;
mod harfrust_backend;
mod itemization;
mod options;
mod run;
mod style;
mod virtual_text;

pub(super) use error::{GlyphRunError, ShapingError};
pub(in crate::text) use options::ShapingDirection;
pub use options::{
    HorizontalDirection, MissingGlyphPolicy, OpenTypeFeature, OpenTypeFeatureTag, ShapingOptions,
};
pub(super) use run::{GlyphRun, ShapedGlyph};

use fallback::shape_segment_with_fallback;
use harfrust_backend::{resolved_direction, shape_glyphs};
#[cfg(test)]
use itemization::ScriptSegment;
use itemization::script_segments;
#[cfg(test)]
use run::validate_placements;

pub(super) fn shape_directional_segment(
    fonts: &[Font],
    text: &str,
    options: &ShapingOptions,
    source_offset: usize,
) -> Result<Vec<GlyphRun>, ShapingError> {
    shape_directional_segment_impl(fonts, text, options, source_offset, None)
}

pub(super) fn shape_directional_segment_in_context(
    fonts: &[Font],
    text: &str,
    options: &ShapingOptions,
    source_offset: usize,
    content: &str,
) -> Result<Vec<GlyphRun>, ShapingError> {
    shape_directional_segment_impl(fonts, text, options, source_offset, Some(content))
}

fn shape_directional_segment_impl(
    fonts: &[Font],
    text: &str,
    options: &ShapingOptions,
    source_offset: usize,
    context: Option<&str>,
) -> Result<Vec<GlyphRun>, ShapingError> {
    if options.script.is_some() {
        return shape_segment_with_fallback(fonts, text, options, source_offset, context);
    }

    let mut grouped_runs = script_segments(text)
        .into_iter()
        .map(|segment| {
            let mut segment_options = options.clone();
            segment_options.script = segment.script;
            shape_segment_with_fallback(
                fonts,
                &text[segment.range.clone()],
                &segment_options,
                source_offset + segment.range.start,
                context,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    if direction_is_backward(resolved_direction(text, options)) {
        grouped_runs.reverse();
    }
    Ok(grouped_runs.into_iter().flatten().collect())
}

const fn direction_is_backward(direction: ShapingDirection) -> bool {
    direction.is_backward()
}

#[cfg(test)]
mod tests;
