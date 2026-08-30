use std::sync::Arc;

use super::super::{GlyphRun, LayoutRun, spacing};
use crate::text::unicode::is_cjk_script;

pub(super) fn opportunity_count(run: &GlyphRun, content: &str) -> usize {
    inter_character_boundaries(run, content).len()
}

pub(super) fn apply_spacing(run: &mut LayoutRun, content: &str, amount: f32) {
    if amount == 0.0 || run.font_size == 0.0 || run.glyphs.len() < 2 {
        return;
    }

    let boundaries = inter_character_boundaries(run, content);
    let spacing_advance = spacing::spacing_advance(
        run,
        spacing::canvas_to_design_units(run, run.font_size, amount),
    );
    let glyphs = Arc::make_mut(&mut run.glyphs);
    for index in boundaries {
        glyphs[index].advance = glyphs[index].advance + spacing_advance;
    }
}

pub(super) fn has_boundary(left: &GlyphRun, right: &GlyphRun, content: &str) -> bool {
    let Some(left_cluster) = left.glyphs.last().map(|glyph| glyph.cluster) else {
        return false;
    };
    let Some(right_cluster) = right.glyphs.first().map(|glyph| glyph.cluster) else {
        return false;
    };
    is_cluster(content, left_cluster) && is_cluster(content, right_cluster)
}

fn inter_character_boundaries(run: &GlyphRun, content: &str) -> Vec<usize> {
    run.glyphs
        .windows(2)
        .enumerate()
        .filter_map(|(index, glyphs)| {
            (glyphs[0].cluster != glyphs[1].cluster
                && is_cluster(content, glyphs[0].cluster)
                && is_cluster(content, glyphs[1].cluster))
            .then_some(index)
        })
        .collect()
}

fn is_cluster(content: &str, cluster: usize) -> bool {
    content
        .get(cluster..)
        .and_then(|suffix| suffix.chars().next())
        .is_some_and(is_cjk_script)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_punctuation_is_not_an_inter_character_opportunity() {
        assert!(is_cluster("日本", 0));
        assert!(is_cluster("日本", "日".len()));
        assert!(!is_cluster("。日", 0));
        assert!(!is_cluster("A日", 0));
    }
}
