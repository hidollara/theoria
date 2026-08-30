use std::{ops::Range, sync::Arc};

use super::super::{PositionedRun, TextLine};
use crate::text::unicode::word_indices;

#[derive(Clone, Copy)]
struct TatweelSlot {
    run_index: usize,
    source_position: usize,
}

struct TatweelWord {
    source_range: Range<usize>,
    slots: Vec<TatweelSlot>,
    cursor: usize,
}

pub(super) fn insert_to_width(
    line: &mut TextLine,
    content: &str,
    width: f32,
    maximum_insertions: usize,
) {
    let mut words = tatweel_words(&line.runs, content);
    if words.is_empty() {
        return;
    }

    let mut insertions = vec![Vec::new(); line.runs.len()];
    let mut insertion_count = 0;
    let mut first_word = 0;
    loop {
        let mut accepted_in_pass = false;
        for offset in 0..words.len() {
            let word_index = (first_word + offset) % words.len();
            let slot_count = words[word_index].slots.len();
            for _ in 0..slot_count {
                if insertion_count == maximum_insertions {
                    return;
                }
                let slot = {
                    let word = &mut words[word_index];
                    let slot = word.slots[word.cursor];
                    word.cursor = (word.cursor + 1) % word.slots.len();
                    slot
                };
                if try_insert(line, content, width, &mut insertions, slot) {
                    insertion_count += 1;
                    accepted_in_pass = true;
                    break;
                }
            }
        }
        if !accepted_in_pass {
            break;
        }
        first_word = (first_word + 1) % words.len();
    }
}

fn try_insert(
    line: &mut TextLine,
    content: &str,
    width: f32,
    insertions: &mut [Vec<usize>],
    slot: TatweelSlot,
) -> bool {
    let run_insertions = &mut insertions[slot.run_index];
    run_insertions.push(slot.source_position);
    let Some(candidate) = line.runs[slot.run_index]
        .run
        .with_tatweels(content, run_insertions)
    else {
        run_insertions.pop();
        return false;
    };
    let old_advance = line.runs[slot.run_index].run.advance().x;
    let candidate =
        super::super::LayoutRun::new(candidate, line.runs[slot.run_index].run.font_size);
    let candidate_advance = candidate.advance().x;
    let line_advance = line.advance.x - old_advance + candidate_advance;
    if line_advance.abs() > line.advance.x.abs() + 0.001 && line_advance.abs() <= width + 0.001 {
        Arc::make_mut(&mut line.runs)[slot.run_index].run = candidate;
        line.advance.x = line_advance;
        true
    } else {
        run_insertions.pop();
        false
    }
}

fn tatweel_words(runs: &[PositionedRun], content: &str) -> Vec<TatweelWord> {
    let word_ranges: Vec<_> = word_indices(content)
        .into_iter()
        .map(|(start, word)| start..start + word.len())
        .collect();
    let mut words: Vec<TatweelWord> = Vec::new();
    for (run_index, positioned) in runs.iter().enumerate() {
        for source_position in positioned.run.tatweel_insertion_positions() {
            let Some(source_range) = word_ranges
                .iter()
                .find(|range| range.contains(&source_position))
            else {
                continue;
            };
            let word_index = words
                .iter()
                .position(|word| word.source_range == *source_range)
                .unwrap_or_else(|| {
                    words.push(TatweelWord {
                        source_range: source_range.clone(),
                        slots: Vec::new(),
                        cursor: 0,
                    });
                    words.len() - 1
                });
            words[word_index].slots.push(TatweelSlot {
                run_index,
                source_position,
            });
        }
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::test_support::windows_test_font;
    use crate::text::{JustificationOptions, Text, TextStyle};

    #[cfg(target_os = "windows")]
    #[test]
    fn arabic_justification_distributes_elongation_across_words() {
        let Some(font) = windows_test_font() else {
            return;
        };
        let content = "\u{0645}\u{0631}\u{062d}\u{0628}\u{0627} \u{0628}\u{0627}\u{0644}\u{0639}\u{0627}\u{0644}\u{0645}";
        let text = Text::horizontal(TextStyle::new(&[font], 24.0))
            .push(content)
            .layout()
            .unwrap();
        let mut line = text.lines[0].line.clone();
        let natural = line.clone();
        let words: Vec<_> = word_indices(content)
            .into_iter()
            .map(|(start, word)| start..start + word.len())
            .collect();
        assert_eq!(words.len(), 2);

        line.justify_to_width(
            content,
            natural.advance().x.abs() + 30.0,
            JustificationOptions::default(),
        );

        for word in words {
            let glyph_count = |line: &TextLine| {
                line.runs
                    .iter()
                    .flat_map(|run| run.run.glyphs.iter())
                    .filter(|glyph| word.contains(&glyph.cluster))
                    .count()
            };
            assert!(glyph_count(&line) > glyph_count(&natural));
        }
    }
}
