use super::{GlyphRun, shape_glyphs};

/// Shapes generated text while keeping every resulting cluster anchored to
/// the caller's original logical text.
pub(super) fn reshape(
    run: &GlyphRun,
    virtual_text: &str,
    cluster_map: &[(usize, usize)],
) -> Option<GlyphRun> {
    let mut glyphs = shape_glyphs(&run.font, virtual_text, &run.options, None);
    if glyphs.iter().any(|glyph| glyph.glyph_id.to_u32() == 0) {
        return None;
    }
    for glyph in &mut glyphs {
        let map_index = cluster_map
            .partition_point(|(virtual_offset, _)| *virtual_offset <= glyph.cluster)
            .saturating_sub(1);
        glyph.cluster = cluster_map.get(map_index)?.1;
    }
    GlyphRun::new(
        run.font.clone(),
        glyphs,
        run.source_range.clone(),
        run.direction,
        run.script,
        run.options.clone(),
    )
    .ok()
}
