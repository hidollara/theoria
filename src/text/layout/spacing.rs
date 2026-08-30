use std::sync::Arc;

use crate::math::Vector2;

use super::{GlyphRun, LayoutRun, ShapingDirection, TabAlignment, TabStop, TabStops};

#[derive(Clone)]
pub(super) struct ResolvedTabStops {
    explicit: TabStops,
    interval: f32,
}

impl ResolvedTabStops {
    pub(super) const fn new(explicit: TabStops, interval: f32) -> Self {
        Self { explicit, interval }
    }

    fn next(&self, inline: f32) -> Option<TabStop> {
        self.explicit
            .as_slice()
            .iter()
            .copied()
            .find(|stop| stop.position() > inline)
    }
}

/// A shaped run together with the inline-spacing policy that layout still
/// needs to apply.
///
/// Shaping remains responsible only for natural glyph positions. This private
/// transport value carries layout policy across script, bidi, and fallback run
/// boundaries until the complete visual sequence is known.
pub(super) struct InlineRun {
    run: LayoutRun,
    spacing: Option<Spacing>,
    tab_stops: Option<ResolvedTabStops>,
    baseline_shift: f32,
}

#[derive(Clone, Copy)]
struct Spacing {
    group: usize,
    letter: f32,
}

impl InlineRun {
    pub(super) fn new(
        mut run: GlyphRun,
        font_size: f32,
        group: usize,
        letter_spacing: f32,
        word_spacing: f32,
        content: &str,
        baseline_shift: f32,
    ) -> Self {
        apply_internal_letter_spacing(&mut run, font_size, content, letter_spacing);
        apply_word_spacing(&mut run, font_size, content, word_spacing);
        Self {
            run: LayoutRun::new(run, font_size),
            spacing: Some(Spacing {
                group,
                letter: letter_spacing,
            }),
            tab_stops: None,
            baseline_shift,
        }
    }

    pub(super) fn tab(run: GlyphRun, font_size: f32, tab_stops: ResolvedTabStops) -> Self {
        Self {
            run: LayoutRun::new(run, font_size),
            spacing: None,
            tab_stops: Some(tab_stops),
            baseline_shift: 0.0,
        }
    }

    #[cfg(test)]
    pub(super) const fn unspaced(run: GlyphRun, font_size: f32) -> Self {
        Self {
            run: LayoutRun::new(run, font_size),
            spacing: None,
            tab_stops: None,
            baseline_shift: 0.0,
        }
    }

    pub(super) fn continues_with(&self, next: &Self) -> bool {
        matches!(
            (self.spacing, next.spacing),
            (Some(left), Some(right)) if left.group == right.group
        )
    }

    pub(super) fn add_trailing_letter_spacing(&mut self) {
        let Some(spacing) = self.spacing else {
            return;
        };
        let font_size = self.run.font_size;
        add_spacing_to_last_glyph(&mut self.run, font_size, spacing.letter);
    }

    pub(super) fn spacing_key(&self) -> Option<(usize, f32)> {
        self.spacing.map(|spacing| (spacing.group, spacing.letter))
    }

    pub(super) const fn baseline_shift(&self) -> f32 {
        self.baseline_shift
    }

    pub(super) const fn is_tab(&self) -> bool {
        self.tab_stops.is_some()
    }

    pub(super) fn has_same_glyph_identities(&self, other: &Self) -> bool {
        self.run.source_range == other.run.source_range
            && self.run.glyphs.len() == other.run.glyphs.len()
            && self
                .run
                .glyphs
                .iter()
                .zip(other.run.glyphs.iter())
                .all(|(left, right)| left.glyph_id == right.glyph_id)
    }

    pub(super) fn tab_advance(
        &self,
        inline: f32,
        field_width: f32,
        decimal_anchor: Option<f32>,
    ) -> Option<f32> {
        let tabs = self.tab_stops.as_ref()?;
        let (position, alignment) = if let Some(stop) = tabs.next(inline) {
            (stop.position(), stop.alignment())
        } else if tabs.interval > 0.0 {
            (
                (inline / tabs.interval)
                    .floor()
                    .mul_add(tabs.interval, tabs.interval),
                TabAlignment::Start,
            )
        } else {
            return Some(0.0);
        };
        let anchor = match alignment {
            TabAlignment::Start => 0.0,
            TabAlignment::End => field_width,
            TabAlignment::Center => field_width * 0.5,
            TabAlignment::Decimal(_) => decimal_anchor.unwrap_or(field_width),
        };
        Some((position - inline - anchor).max(0.0))
    }

    pub(super) fn decimal_character(&self, inline: f32) -> Option<char> {
        let tabs = self.tab_stops.as_ref()?;
        match tabs.next(inline)?.alignment() {
            TabAlignment::Decimal(character) => Some(character),
            _ => None,
        }
    }

    pub(super) fn natural_horizontal_advance(&self) -> f32 {
        self.run.advance().x.abs()
    }

    pub(super) fn natural_vertical_advance(&self) -> f32 {
        self.run.advance().y.abs()
    }

    pub(super) fn decimal_anchor(&self, content: &str, character: char) -> Option<f32> {
        let range = self.run.source_range.clone();
        let target = range.start + content.get(range)?.find(character)?;
        let scale = self.run.font_size / f32::from(self.run.font.metrics().units_per_em());
        let mut advance = 0.0;
        for glyph in self.run.glyphs.iter() {
            if glyph.cluster == target {
                return Some(advance);
            }
            advance += glyph.advance.x.abs() * scale;
        }
        None
    }

    pub(super) fn set_horizontal_advance(&mut self, advance: f32) {
        let design_advance = if self.run.font_size == 0.0 {
            0.0
        } else {
            advance * f32::from(self.run.font.metrics().units_per_em()) / self.run.font_size
        };
        let glyphs = Arc::make_mut(&mut self.run.glyphs);
        for glyph in glyphs.iter_mut() {
            glyph.advance = Vector2::ZERO;
        }
        if let Some(glyph) = glyphs.last_mut() {
            glyph.advance = Vector2::new(design_advance, 0.0);
        }
    }

    pub(super) fn into_inner(self) -> LayoutRun {
        self.run
    }
}

/// Applies tracking at cluster boundaries already resolved by shaping.
/// Required ligatures remain indivisible because they retain one cluster.
fn apply_internal_letter_spacing(run: &mut GlyphRun, font_size: f32, content: &str, spacing: f32) {
    if spacing == 0.0 || font_size == 0.0 || run.glyphs.len() < 2 {
        return;
    }

    let spacing_advance = spacing_advance(run, canvas_to_design_units(run, font_size, spacing));
    let boundaries: Vec<_> = run
        .glyphs
        .windows(2)
        .enumerate()
        .filter_map(|(index, glyphs)| {
            (glyphs[0].cluster != glyphs[1].cluster
                && allows_letter_spacing_boundary(
                    run,
                    content,
                    glyphs[0].cluster,
                    glyphs[1].cluster,
                ))
            .then_some(index)
        })
        .collect();
    let glyphs = Arc::make_mut(&mut run.glyphs);
    for index in boundaries {
        glyphs[index].advance = glyphs[index].advance + spacing_advance;
    }
}

fn allows_letter_spacing_boundary(
    run: &GlyphRun,
    content: &str,
    left_cluster: usize,
    right_cluster: usize,
) -> bool {
    if !run.script().is_some_and(is_cursive_script) {
        return true;
    }
    [left_cluster, right_cluster].into_iter().any(|cluster| {
        content
            .get(cluster..)
            .and_then(|suffix| suffix.chars().next())
            .is_some_and(is_word_separator)
    })
}

fn is_cursive_script(script: [u8; 4]) -> bool {
    matches!(
        &script,
        b"Arab" | b"Rohg" | b"Mand" | b"Mong" | b"Nkoo" | b"Phag" | b"Syrc"
    )
}

/// Expands the intrinsic advance of explicit word-separator clusters without
/// attempting linguistic word segmentation. Visible separator glyphs remain
/// centered in their enlarged advance.
pub(super) fn apply_word_spacing(run: &mut GlyphRun, font_size: f32, content: &str, spacing: f32) {
    if spacing == 0.0 || font_size == 0.0 || run.glyphs.is_empty() {
        return;
    }

    let clusters = word_separator_clusters(run, content);
    let spacing_advance = spacing_advance(run, canvas_to_design_units(run, font_size, spacing));
    let glyphs = Arc::make_mut(&mut run.glyphs);
    for (start, end) in clusters {
        for glyph in &mut glyphs[start..end] {
            glyph.offset = glyph.offset - spacing_advance * 0.5;
        }
        glyphs[end - 1].advance = glyphs[end - 1].advance + spacing_advance;
    }
}

pub(super) fn word_separator_count(run: &GlyphRun, content: &str) -> usize {
    word_separator_clusters(run, content).len()
}

fn word_separator_clusters(run: &GlyphRun, content: &str) -> Vec<(usize, usize)> {
    let mut clusters = Vec::new();
    let mut start = 0;
    while start < run.glyphs.len() {
        let cluster = run.glyphs[start].cluster;
        let mut end = start + 1;
        while end < run.glyphs.len() && run.glyphs[end].cluster == cluster {
            end += 1;
        }
        let is_separator = content
            .get(cluster..)
            .and_then(|suffix| suffix.chars().next())
            .is_some_and(is_word_separator);
        if is_separator && cluster_has_inline_advance(run, start, end) {
            clusters.push((start, end));
        }
        start = end;
    }
    clusters
}

fn cluster_has_inline_advance(run: &GlyphRun, start: usize, end: usize) -> bool {
    let advance = run.glyphs[start..end]
        .iter()
        .fold(Vector2::ZERO, |sum, glyph| sum + glyph.advance);
    match run.direction {
        ShapingDirection::LeftToRight | ShapingDirection::RightToLeft => advance.x != 0.0,
        ShapingDirection::TopToBottom | ShapingDirection::BottomToTop => advance.y != 0.0,
    }
}

fn is_word_separator(character: char) -> bool {
    matches!(
        character,
        '\u{0020}' // SPACE
            | '\u{00A0}' // NO-BREAK SPACE
            | '\u{1361}' // ETHIOPIC WORDSPACE
            | '\u{1680}' // OGHAM SPACE MARK
            | '\u{200B}' // ZERO WIDTH SPACE; ignored when its advance is zero
            | '\u{10100}' // AEGEAN WORD SEPARATOR LINE
            | '\u{10101}' // AEGEAN WORD SEPARATOR DOT
            | '\u{1039F}' // UGARITIC WORD DIVIDER
            | '\u{1091F}' // PHOENICIAN WORD SEPARATOR
    )
}

pub(super) fn add_spacing_to_last_glyph(run: &mut GlyphRun, font_size: f32, spacing: f32) {
    if spacing == 0.0 || font_size == 0.0 {
        return;
    }
    let spacing_advance = spacing_advance(run, canvas_to_design_units(run, font_size, spacing));
    if let Some(glyph) = Arc::make_mut(&mut run.glyphs).last_mut() {
        glyph.advance = glyph.advance + spacing_advance;
    }
}

pub(super) fn canvas_to_design_units(run: &GlyphRun, font_size: f32, spacing: f32) -> f32 {
    spacing * f32::from(run.font.metrics().units_per_em()) / font_size
}

pub(super) fn spacing_advance(run: &GlyphRun, spacing: f32) -> Vector2 {
    let advance = run
        .glyphs
        .iter()
        .fold(Vector2::ZERO, |sum, glyph| sum + glyph.advance);
    match run.direction {
        ShapingDirection::LeftToRight | ShapingDirection::RightToLeft => {
            Vector2::new(spacing * nonzero_sign(advance.x, 1.0).signum(), 0.0)
        }
        ShapingDirection::TopToBottom | ShapingDirection::BottomToTop => {
            Vector2::new(0.0, spacing * nonzero_sign(advance.y, -1.0).signum())
        }
    }
}

fn nonzero_sign(value: f32, fallback: f32) -> f32 {
    if value == 0.0 { fallback } else { value }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_separators_are_distinct_from_fixed_width_spaces_and_punctuation() {
        assert!(is_word_separator(' '));
        assert!(is_word_separator('\u{00A0}'));
        assert!(is_word_separator('\u{1361}'));
        assert!(!is_word_separator('\u{3000}'));
        assert!(!is_word_separator(','));
    }

    #[test]
    fn known_cursive_scripts_are_kept_out_of_inter_letter_spacing() {
        for script in [
            *b"Arab", *b"Rohg", *b"Mand", *b"Mong", *b"Nkoo", *b"Phag", *b"Syrc",
        ] {
            assert!(is_cursive_script(script));
        }
        assert!(!is_cursive_script(*b"Latn"));
        assert!(!is_cursive_script(*b"Deva"));
    }
}
