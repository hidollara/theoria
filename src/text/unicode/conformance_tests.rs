use unicode_bidi::{Level, ParagraphBidiInfo};

use super::{bidi_info, grapheme_boundaries, line_breaks, word_boundaries};
use crate::text::LineBreakStyle;

const GRAPHEME_BREAK_TEST: &str =
    include_str!("../../../tests/data/unicode/17.0.0/GraphemeBreakTest.txt");
const WORD_BREAK_TEST: &str = include_str!("../../../tests/data/unicode/17.0.0/WordBreakTest.txt");
const LINE_BREAK_TEST: &str = include_str!("../../../tests/data/unicode/17.0.0/LineBreakTest.txt");
const BIDI_CHARACTER_TEST: &str =
    include_str!("../../../tests/data/unicode/17.0.0/BidiCharacterTest.txt");
const BIDI_TEST: &str = include_str!("../../../tests/data/unicode/17.0.0/BidiTest.txt");

fn boundary_case(line: &str) -> Option<(String, Vec<usize>)> {
    let data = line.split('#').next()?.trim();
    if data.is_empty() {
        return None;
    }
    let mut text = String::new();
    let mut expected = Vec::new();
    for token in data.split_ascii_whitespace() {
        match token {
            "÷" => expected.push(text.len()),
            "×" => {}
            code_point => text.push(char::from_u32(u32::from_str_radix(code_point, 16).ok()?)?),
        }
    }
    Some((text, expected))
}

fn assert_boundaries(name: &str, data: &str, boundaries: impl Fn(&str) -> Vec<usize>) {
    for (line_index, line) in data.lines().enumerate() {
        let Some((text, expected)) = boundary_case(line) else {
            continue;
        };
        assert_eq!(
            boundaries(&text),
            expected,
            "{name} failed at official data line {}",
            line_index + 1
        );
    }
}

#[test]
fn uax29_extended_grapheme_boundaries_conform_to_unicode_17() {
    assert_boundaries(
        "GraphemeBreakTest",
        GRAPHEME_BREAK_TEST,
        grapheme_boundaries,
    );
}

#[test]
fn uax29_default_word_boundaries_conform_to_unicode_17() {
    assert_boundaries("WordBreakTest", WORD_BREAK_TEST, word_boundaries);
}

#[test]
fn uax14_default_line_boundaries_conform_to_unicode_17() {
    assert_boundaries("LineBreakTest", LINE_BREAK_TEST, |text| {
        line_breaks(text, LineBreakStyle::Strict, None)
            .into_iter()
            .map(|(index, _)| index)
            .collect()
    });
}

fn code_points(field: &str) -> Vec<char> {
    field
        .split_ascii_whitespace()
        .map(|code_point| char::from_u32(u32::from_str_radix(code_point, 16).unwrap()).unwrap())
        .collect()
}

fn expected_levels(field: &str) -> Vec<Option<u8>> {
    field
        .split_ascii_whitespace()
        .map(|level| (level != "x").then(|| level.parse().unwrap()))
        .collect()
}

fn numbers(field: &str) -> Vec<usize> {
    field
        .split_ascii_whitespace()
        .map(|number| number.parse().unwrap())
        .collect()
}

fn level_for_mode(mode: u8) -> Option<Level> {
    match mode {
        0 => Some(Level::ltr()),
        1 => Some(Level::rtl()),
        2 => None,
        _ => unreachable!(),
    }
}

fn assert_bidi_case(
    text: &str,
    default_level: Option<Level>,
    expected_paragraph_level: Option<u8>,
    levels: &[Option<u8>],
    expected_order: &[usize],
    context: &str,
) {
    let info = bidi_info(text, default_level);
    if let Some(expected) = expected_paragraph_level {
        assert_eq!(
            info.paragraph_level.number(),
            expected,
            "{context}: paragraph level"
        );
    }
    let character_levels: Vec<_> = info
        .reordered_levels_per_char(0..text.len())
        .into_iter()
        .map(|level| level.number())
        .collect();
    for (index, expected) in levels.iter().copied().enumerate() {
        if let Some(expected) = expected {
            assert_eq!(
                character_levels[index], expected,
                "{context}: level at {index}"
            );
        }
    }
    let retained: Vec<_> = levels
        .iter()
        .enumerate()
        .filter_map(|(index, level)| level.map(|_| index))
        .collect();
    let retained_levels: Vec<_> = retained
        .iter()
        .map(|index| Level::new(character_levels[*index]).unwrap())
        .collect();
    let actual_order: Vec<_> = ParagraphBidiInfo::reorder_visual(&retained_levels)
        .into_iter()
        .map(|index| retained[index])
        .collect();
    assert_eq!(actual_order, expected_order, "{context}: visual order");
}

#[test]
fn uax9_character_cases_conform_to_unicode_17() {
    for (line_index, line) in BIDI_CHARACTER_TEST.lines().enumerate() {
        let data = line.split('#').next().unwrap().trim();
        if data.is_empty() {
            continue;
        }
        let fields: Vec<_> = data.split(';').map(str::trim).collect();
        let characters = code_points(fields[0]);
        let text: String = characters.iter().collect();
        let mode: u8 = fields[1].parse().unwrap();
        assert_bidi_case(
            &text,
            level_for_mode(mode),
            Some(fields[2].parse().unwrap()),
            &expected_levels(fields[3]),
            &numbers(fields[4]),
            &format!("BidiCharacterTest line {}", line_index + 1),
        );
    }
}

fn representative(bidi_class: &str) -> char {
    match bidi_class {
        "L" => 'a',
        "R" => '\u{05d0}',
        "AL" => '\u{0627}',
        "EN" => '0',
        "ES" => '+',
        "ET" => '$',
        "AN" => '\u{0660}',
        "CS" => ',',
        "B" => '\u{2029}',
        "S" => '\t',
        "WS" => ' ',
        "ON" => '!',
        "LRE" => '\u{202a}',
        "LRO" => '\u{202d}',
        "RLE" => '\u{202b}',
        "RLO" => '\u{202e}',
        "PDF" => '\u{202c}',
        "NSM" => '\u{0300}',
        "BN" => '\u{00ad}',
        "LRI" => '\u{2066}',
        "RLI" => '\u{2067}',
        "FSI" => '\u{2068}',
        "PDI" => '\u{2069}',
        unknown => panic!("unhandled bidi class {unknown}"),
    }
}

#[test]
fn uax9_class_sequences_conform_to_unicode_17() {
    let mut levels = Vec::new();
    let mut order = Vec::new();
    for (line_index, line) in BIDI_TEST.lines().enumerate() {
        let data = line.split('#').next().unwrap().trim();
        if let Some(value) = data.strip_prefix("@Levels:") {
            levels = expected_levels(value.trim());
            continue;
        }
        if let Some(value) = data.strip_prefix("@Reorder:") {
            order = numbers(value.trim());
            continue;
        }
        if data.is_empty() || data.starts_with('@') {
            continue;
        }
        let (classes, bitset) = data.split_once(';').unwrap();
        let text: String = classes
            .split_ascii_whitespace()
            .map(representative)
            .collect();
        let bitset = u8::from_str_radix(bitset.trim(), 16).unwrap();
        for (bit, mode) in [(1, 2), (2, 0), (4, 1)] {
            if bitset & bit == 0 {
                continue;
            }
            assert_bidi_case(
                &text,
                level_for_mode(mode),
                None,
                &levels,
                &order,
                &format!("BidiTest line {}, mode {mode}", line_index + 1),
            );
        }
    }
}
