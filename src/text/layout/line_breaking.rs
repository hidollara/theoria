use std::{ops::Range, sync::Arc};

pub(super) use crate::text::unicode::LineBreakOpportunity as BreakOpportunity;
use crate::text::unicode::{grapheme_count, grapheme_indices, line_breaks, word_indices};

use super::TextLine;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineBreakStyle {
    Strict,
    #[default]
    Normal,
    Loose,
}

/// Supplies language-aware discretionary breaks for one word.
///
/// Returned indices are UTF-8 byte boundaries within `word`. Invalid and edge
/// indices are ignored by layout. Dictionary loading and caching remain the
/// provider's responsibility.
pub trait Hyphenator: Send + Sync {
    fn hyphenate(&self, word: &str, language: &str) -> Vec<HyphenationOpportunity>;
}

/// Constraints applied to dictionary-provided hyphenation points.
/// Explicit soft hyphens are author intent and are not filtered by these
/// limits. Counts use Unicode grapheme clusters rather than UTF-8 bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HyphenationOptions {
    minimum_prefix: usize,
    minimum_suffix: usize,
    maximum_consecutive_lines: Option<usize>,
}

impl HyphenationOptions {
    pub const fn new() -> Self {
        Self {
            minimum_prefix: 2,
            minimum_suffix: 2,
            maximum_consecutive_lines: Some(2),
        }
    }

    pub const fn with_minimum_prefix(mut self, graphemes: usize) -> Self {
        self.minimum_prefix = graphemes;
        self
    }

    pub const fn with_minimum_suffix(mut self, graphemes: usize) -> Self {
        self.minimum_suffix = graphemes;
        self
    }

    pub const fn with_maximum_consecutive_lines(mut self, lines: Option<usize>) -> Self {
        self.maximum_consecutive_lines = lines;
        self
    }

    pub const fn maximum_consecutive_lines(self) -> Option<usize> {
        self.maximum_consecutive_lines
    }
}

impl Default for HyphenationOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Hyphenator + ?Sized> Hyphenator for Arc<T> {
    fn hyphenate(&self, word: &str, language: &str) -> Vec<HyphenationOpportunity> {
        (**self).hyphenate(word, language)
    }
}

/// One dictionary-provided break inside a word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HyphenationOpportunity {
    byte_index: usize,
    mark: HyphenationMark,
}

impl HyphenationOpportunity {
    pub const fn new(byte_index: usize, mark: HyphenationMark) -> Self {
        Self { byte_index, mark }
    }

    pub const fn byte_index(self) -> usize {
        self.byte_index
    }

    pub const fn mark(self) -> HyphenationMark {
        self.mark
    }
}

/// Whether a dictionary break draws a hyphen or only permits a boundary.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HyphenationMark {
    None,
    #[default]
    Hyphen,
}

/// Policy for a grapheme sequence that has no ordinary fitting break.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OverflowWrap {
    #[default]
    Normal,
    BreakWord,
}

/// How a paragraph chooses among already-valid break opportunities.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineBreakStrategy {
    /// Fast, stable behavior suitable for frequently changing UI labels.
    #[default]
    Greedy,
    /// Minimizes raggedness across the complete paragraph.
    Balanced,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct BreakCandidate {
    pub(super) index: usize,
    pub(super) opportunity: BreakOpportunity,
    pub(super) show_hyphen: bool,
    pub(super) automatic_hyphen: bool,
    pub(super) emergency: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct HyphenationBoundary {
    pub(super) index: usize,
    pub(super) show_hyphen: bool,
    pub(super) automatic: bool,
}

/// Enumerates default UAX #14 break opportunities as UTF-8 byte boundaries.
pub(super) fn opportunities(
    text: &str,
    style: LineBreakStyle,
    language: Option<&str>,
) -> impl Iterator<Item = (usize, BreakOpportunity)> {
    line_breaks(text, style, language).into_iter()
}

pub(super) fn candidates(
    text: &str,
    automatic: &[HyphenationBoundary],
    style: LineBreakStyle,
    language: Option<&str>,
    overflow_wrap: OverflowWrap,
) -> Vec<BreakCandidate> {
    let mut candidates: Vec<_> = opportunities(text, style, language)
        .map(|(index, opportunity)| BreakCandidate {
            index,
            opportunity,
            show_hyphen: opportunity == BreakOpportunity::Allowed
                && text[..index].ends_with('\u{00ad}'),
            automatic_hyphen: false,
            emergency: false,
        })
        .collect();
    for boundary in automatic {
        if boundary.index == 0
            || boundary.index >= text.len()
            || !text.is_char_boundary(boundary.index)
        {
            continue;
        }
        if let Some(candidate) = candidates
            .iter_mut()
            .find(|candidate| candidate.index == boundary.index)
        {
            candidate.show_hyphen |= boundary.show_hyphen;
            candidate.automatic_hyphen |= boundary.automatic;
        } else {
            candidates.push(BreakCandidate {
                index: boundary.index,
                opportunity: BreakOpportunity::Allowed,
                show_hyphen: boundary.show_hyphen,
                automatic_hyphen: boundary.automatic,
                emergency: false,
            });
        }
    }
    if overflow_wrap == OverflowWrap::BreakWord {
        for (index, _) in grapheme_indices(text).into_iter().skip(1) {
            if !candidates.iter().any(|candidate| candidate.index == index) {
                candidates.push(BreakCandidate {
                    index,
                    opportunity: BreakOpportunity::Allowed,
                    show_hyphen: false,
                    automatic_hyphen: false,
                    emergency: true,
                });
            }
        }
    }
    candidates.sort_by_key(|candidate| candidate.index);
    candidates
}

pub(super) fn provider_boundaries(
    text: &str,
    language: &str,
    hyphenator: &dyn Hyphenator,
    options: HyphenationOptions,
) -> Vec<HyphenationBoundary> {
    let mut boundaries = Vec::new();
    for (word_start, word) in word_indices(text) {
        if word.contains('\u{00ad}') {
            continue;
        }
        for opportunity in hyphenator.hyphenate(word, language) {
            let offset = opportunity.byte_index();
            if offset == 0 || offset >= word.len() || !word.is_char_boundary(offset) {
                continue;
            }
            let prefix = grapheme_count(&word[..offset]);
            let suffix = grapheme_count(&word[offset..]);
            if prefix < options.minimum_prefix || suffix < options.minimum_suffix {
                continue;
            }
            boundaries.push(HyphenationBoundary {
                index: word_start + offset,
                show_hyphen: opportunity.mark() == HyphenationMark::Hyphen,
                automatic: true,
            });
        }
    }
    boundaries.sort_by_key(|boundary| boundary.index);
    boundaries.dedup();
    boundaries
}

/// Splits logical text at mandatory Unicode breaks while excluding the break
/// controls themselves from the ranges passed to shaping.
pub(super) fn paragraph_ranges(text: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut start = 0;
    for (end, opportunity) in opportunities(text, LineBreakStyle::Strict, None) {
        if opportunity != BreakOpportunity::Mandatory {
            continue;
        }
        let content_end = hard_break_start(text, start, end);
        ranges.push(start..content_end);
        let ended_with_hard_break = content_end < end;
        start = end;
        if end == text.len() && ended_with_hard_break {
            ranges.push(end..end);
        }
    }
    if ranges.is_empty() {
        ranges.push(0..0);
    }
    ranges
}

fn hard_break_start(text: &str, start: usize, mut end: usize) -> usize {
    while end > start {
        let Some(character) = text[..end].chars().next_back() else {
            break;
        };
        if !matches!(
            character,
            '\r' | '\n' | '\u{000b}' | '\u{000c}' | '\u{0085}' | '\u{2028}' | '\u{2029}'
        ) {
            break;
        }
        end -= character.len_utf8();
    }
    end
}

fn hyphenated_run(
    line: &TextLine,
    content: &str,
    break_index: usize,
) -> Option<(usize, super::GlyphRun)> {
    let soft_hyphen = content
        .get(..break_index)
        .and_then(|prefix| prefix.char_indices().next_back())
        .filter(|(_, character)| *character == '\u{00ad}')
        .map(|(index, _)| index);
    let run_index = soft_hyphen.map_or_else(
        || {
            line.runs
                .iter()
                .rposition(|positioned| positioned.run.source_range.end == break_index)
        },
        |soft_hyphen| {
            line.runs
                .iter()
                .position(|positioned| positioned.run.source_range.contains(&soft_hyphen))
        },
    );
    let run_index = run_index?;
    let run = if let Some(soft_hyphen) = soft_hyphen {
        line.runs[run_index]
            .run
            .with_visible_soft_hyphen(content, soft_hyphen)
    } else {
        line.runs[run_index]
            .run
            .with_inserted_hyphen(content, break_index)
    };
    Some((run_index, run?))
}

/// Materializes a discretionary hyphen without changing the logical source.
pub(super) fn realize_horizontal_hyphen(
    line: &mut TextLine,
    content: &str,
    break_index: usize,
) -> bool {
    let Some((run_index, run)) = hyphenated_run(line, content, break_index) else {
        return false;
    };
    Arc::make_mut(&mut line.runs)[run_index]
        .run
        .replace_shaped(run);
    line.reflow_horizontal_runs();
    true
}

pub(super) fn realize_vertical_hyphen(
    line: &mut TextLine,
    content: &str,
    break_index: usize,
) -> bool {
    let Some((run_index, run)) = hyphenated_run(line, content, break_index) else {
        return false;
    };
    line.replace_vertical_run(run_index, run)
}

#[cfg(test)]
mod tests;
