use std::collections::HashMap;

use crate::{
    font::{Font, ResolvedFont},
    text::unicode::grapheme_indices,
};

use super::{
    GlyphRun, GlyphRunError, MissingGlyphPolicy, ShapingError, ShapingOptions,
    harfrust_backend::{resolved_direction, shape_glyphs},
};

pub(super) fn shape_segment_with_fallback(
    fonts: &[Font],
    text: &str,
    options: &ShapingOptions,
    source_offset: usize,
    context: Option<&str>,
) -> Result<Vec<GlyphRun>, ShapingError> {
    let resolved_fonts: Vec<_> = fonts.iter().map(Font::resolve).collect();
    let shaping_context = context.map(|content| {
        (
            content,
            source_offset..source_offset.saturating_add(text.len()),
        )
    });
    let primary_glyphs = shape_glyphs(&resolved_fonts[0], text, options, shaping_context);
    if primary_glyphs
        .iter()
        .all(|glyph| glyph.glyph_id.to_u32() != 0)
    {
        return Ok(vec![
            GlyphRun::new(
                resolved_fonts[0].clone(),
                primary_glyphs,
                0..text.len(),
                resolved_direction(text, options),
                options.script,
                options.clone(),
            )?
            .with_source_offset(source_offset),
        ]);
    }
    let missing_primary_clusters: Vec<_> = primary_glyphs
        .iter()
        .filter(|glyph| glyph.glyph_id.to_u32() == 0)
        .map(|glyph| glyph.cluster)
        .collect();
    let replacement_font =
        (options.missing_glyph_policy == MissingGlyphPolicy::Replace).then(|| {
            resolved_fonts
                .iter()
                .position(|font| font_supports_segment(font, "\u{fffd}", options))
                .unwrap_or(0)
        });

    let mut segments: Vec<FallbackSegment> = Vec::new();
    let mut support_cache: HashMap<(usize, &str), bool> = HashMap::new();
    for (start, grapheme) in grapheme_indices(text) {
        let font_index = resolved_fonts
            .iter()
            .enumerate()
            .position(|(font_index, font)| {
                if font_index == 0 && missing_primary_clusters.contains(&start) {
                    return false;
                }
                *support_cache
                    .entry((font_index, grapheme))
                    .or_insert_with(|| font_supports_segment(font, grapheme, options))
            });
        let kind = match font_index {
            Some(font_index) => FallbackSegmentKind::Font(font_index),
            None if options.missing_glyph_policy == MissingGlyphPolicy::Replace => {
                FallbackSegmentKind::Replacement(
                    replacement_font.expect("replacement font is resolved for Replace policy"),
                )
            }
            None => {
                return Err(ShapingError::MissingGlyph {
                    byte_index: source_offset + start,
                });
            }
        };
        let end = start + grapheme.len();
        if let Some(segment) = segments.last_mut()
            && segment.kind == kind
            && !matches!(kind, FallbackSegmentKind::Replacement(_))
        {
            segment.end = end;
        } else {
            segments.push(FallbackSegment { kind, start, end });
        }
    }

    let mut runs = segments
        .into_iter()
        .map(|segment| match segment.kind {
            FallbackSegmentKind::Font(font_index) => GlyphRun::shape_with_context(
                resolved_fonts[font_index].clone(),
                &text[segment.start..segment.end],
                options,
                context.map(|content| {
                    (
                        content,
                        source_offset + segment.start..source_offset + segment.end,
                    )
                }),
            )
            .map(|run| run.with_source_offset(source_offset + segment.start)),
            FallbackSegmentKind::Replacement(font_index) => shape_replacement(
                &resolved_fonts[font_index],
                &text[segment.start..segment.end],
                options,
                source_offset + segment.start,
                source_offset + segment.end,
            ),
        })
        .collect::<Result<Vec<_>, _>>()?;
    if resolved_direction(text, options).is_backward() {
        runs.reverse();
    }
    Ok(runs)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FallbackSegmentKind {
    Font(usize),
    Replacement(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FallbackSegment {
    kind: FallbackSegmentKind,
    start: usize,
    end: usize,
}

fn shape_replacement(
    font: &ResolvedFont,
    original: &str,
    options: &ShapingOptions,
    source_start: usize,
    source_end: usize,
) -> Result<GlyphRun, GlyphRunError> {
    let mut glyphs = shape_glyphs(font, "\u{fffd}", options, None);
    for glyph in &mut glyphs {
        glyph.cluster = source_start;
    }
    GlyphRun::new(
        font.clone(),
        glyphs,
        source_start..source_end,
        resolved_direction(original, options),
        options.script,
        options.clone(),
    )
}

fn font_supports_segment(font: &ResolvedFont, text: &str, options: &ShapingOptions) -> bool {
    let mut characters = text.chars();
    if let Some(character) = characters.next()
        && characters.next().is_none()
        && font.has_nominal_glyph(character)
    {
        return true;
    }
    shape_glyphs(font, text, options, None)
        .iter()
        .all(|glyph| glyph.glyph_id.to_u32() != 0)
}
