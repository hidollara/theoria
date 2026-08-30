//! Unicode algorithms and property data used by the text domain.
//!
//! ICU4X is intentionally kept behind this private boundary. Theoria's text
//! model deals in UTF-8 offsets and its own semantic types; changing the data
//! provider must not leak ICU types into the public API.

use std::borrow::Cow;

use icu_locale_core::LanguageIdentifier;
use icu_properties::{
    CodePointMapData,
    props::{
        BidiClass, BidiMirroringGlyph, BidiPairedBracketType, GeneralCategory, LineBreak,
        NamedEnumeratedProperty, Script, VerticalOrientation,
    },
    script::ScriptWithExtensions,
};
use icu_segmenter::{
    GraphemeClusterSegmenter, LineSegmenter, WordSegmenter,
    options::{LineBreakOptions, LineBreakStrictness, WordBreakInvariantOptions},
};
use unicode_bidi::{Level, ParagraphBidiInfo};

use super::LineBreakStyle;

pub(super) fn bidi_info(text: &str, default_level: Option<Level>) -> ParagraphBidiInfo<'_> {
    let classes = CodePointMapData::<BidiClass>::new();
    ParagraphBidiInfo::new_with_data_source(&classes, text, default_level)
}

pub(super) fn grapheme_indices(text: &str) -> Vec<(usize, &str)> {
    grapheme_boundaries(text)
        .windows(2)
        .map(|bounds| (bounds[0], &text[bounds[0]..bounds[1]]))
        .collect()
}

fn grapheme_boundaries(text: &str) -> Vec<usize> {
    GraphemeClusterSegmenter::new().segment_str(text).collect()
}

pub(super) fn grapheme_count(text: &str) -> usize {
    GraphemeClusterSegmenter::new()
        .segment_str(text)
        .count()
        .saturating_sub(1)
}

pub(super) fn word_indices(text: &str) -> Vec<(usize, &str)> {
    let boundaries: Vec<_> = WordSegmenter::new_auto(WordBreakInvariantOptions::default())
        .segment_str(text)
        .iter_with_word_type()
        .collect();
    boundaries
        .windows(2)
        .filter_map(|bounds| {
            bounds[1]
                .1
                .is_word_like()
                .then_some((bounds[0].0, &text[bounds[0].0..bounds[1].0]))
        })
        .collect()
}

#[cfg(test)]
fn word_boundaries(text: &str) -> Vec<usize> {
    WordSegmenter::new_auto(WordBreakInvariantOptions::default())
        .segment_str(text)
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LineBreakOpportunity {
    Allowed,
    Mandatory,
}

pub(super) fn line_breaks(
    text: &str,
    style: LineBreakStyle,
    language: Option<&str>,
) -> Vec<(usize, LineBreakOpportunity)> {
    let locale = language.and_then(|language| language.parse::<LanguageIdentifier>().ok());
    let mut options = LineBreakOptions::default();
    options.strictness = Some(match style {
        LineBreakStyle::Strict => LineBreakStrictness::Strict,
        LineBreakStyle::Normal => LineBreakStrictness::Normal,
        LineBreakStyle::Loose => LineBreakStrictness::Loose,
    });
    options.content_locale = locale.as_ref();
    let mut segmenter = LineSegmenter::new_17_for_non_complex_scripts(options);
    let complex_context = locale
        .as_ref()
        .is_some_and(|locale| matches!(locale.language.as_str(), "th" | "lo" | "km" | "my"));
    if complex_context {
        segmenter.load_lstm();
    }
    let segmented_text = if complex_context {
        Cow::Borrowed(text)
    } else {
        Cow::Owned(
            text.chars()
                .map(default_line_break_character)
                .collect::<String>(),
        )
    };
    segmenter
        .segment_str(&segmented_text)
        .filter(|index| *index != 0)
        .map(|index| {
            let opportunity = if index == text.len() || ends_with_hard_break(&text[..index]) {
                LineBreakOpportunity::Mandatory
            } else {
                LineBreakOpportunity::Allowed
            };
            (index, opportunity)
        })
        .collect()
}

fn default_line_break_character(character: char) -> char {
    if CodePointMapData::<LineBreak>::new().get(character) != LineBreak::ComplexContext {
        return character;
    }
    let mark = matches!(
        CodePointMapData::<GeneralCategory>::new().get(character),
        GeneralCategory::NonspacingMark | GeneralCategory::SpacingMark
    );
    match (mark, character.len_utf8()) {
        (true, 2) => '\u{0300}',
        (true, 3) => '\u{081c}',
        (true, 4) => '\u{101fd}',
        (false, 2) => '\u{00c0}',
        (false, 3) => '\u{0800}',
        (false, 4) => '\u{10000}',
        _ => character,
    }
}

fn ends_with_hard_break(text: &str) -> bool {
    text.chars().next_back().is_some_and(|character| {
        matches!(
            CodePointMapData::<LineBreak>::new().get(character),
            LineBreak::MandatoryBreak
                | LineBreak::CarriageReturn
                | LineBreak::LineFeed
                | LineBreak::NextLine
        )
    })
}

pub(super) fn script_extensions(character: char) -> Vec<[u8; 4]> {
    ScriptWithExtensions::new()
        .get_script_extensions_val(character)
        .iter()
        .filter(|script| {
            !matches!(
                *script,
                Script::Common | Script::Inherited | Script::Unknown
            )
        })
        .filter_map(|script| script.short_name().as_bytes().try_into().ok())
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PairedBracket {
    Open(char),
    Close(char),
}

pub(super) fn paired_bracket(character: char) -> Option<PairedBracket> {
    let property = CodePointMapData::<BidiMirroringGlyph>::new().get(character);
    let paired = property.mirroring_glyph?;
    match property.paired_bracket_type {
        BidiPairedBracketType::Open => Some(PairedBracket::Open(paired)),
        BidiPairedBracketType::Close => Some(PairedBracket::Close(paired)),
        _ => None,
    }
}

pub(super) fn is_cjk_script(character: char) -> bool {
    matches!(
        ScriptWithExtensions::new().get_script_val(character),
        Script::Han
            | Script::Hiragana
            | Script::Katakana
            | Script::Bopomofo
            | Script::Hangul
            | Script::Yi
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum VerticalGlyphOrientation {
    Upright,
    Rotated,
    TransformedUpright,
    TransformedRotated,
}

pub(super) fn vertical_glyph_orientation(grapheme: &str) -> VerticalGlyphOrientation {
    if grapheme.chars().any(|character| {
        CodePointMapData::<GeneralCategory>::new().get(character) == GeneralCategory::EnclosingMark
    }) {
        return VerticalGlyphOrientation::Upright;
    }
    match grapheme
        .chars()
        .next()
        .map(|character| CodePointMapData::<VerticalOrientation>::new().get(character))
    {
        Some(VerticalOrientation::Upright) => VerticalGlyphOrientation::Upright,
        Some(VerticalOrientation::TransformedUpright) => {
            VerticalGlyphOrientation::TransformedUpright
        }
        Some(VerticalOrientation::TransformedRotated) => {
            VerticalGlyphOrientation::TransformedRotated
        }
        _ => VerticalGlyphOrientation::Rotated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graphemes_use_extended_cluster_boundaries() {
        assert_eq!(grapheme_indices("a\u{0301}b"), [(0, "a\u{0301}"), (3, "b")]);
    }

    #[test]
    fn word_indices_exclude_interword_segments() {
        assert_eq!(word_indices("one, two"), [(0, "one"), (5, "two")]);
    }

    #[test]
    fn bidi_uses_icu_property_data() {
        let info = bidi_info("abc אבג", None);
        assert!(!info.is_pure_ltr);
        assert!(info.levels[4].is_rtl());
    }

    #[test]
    fn script_extensions_preserve_multi_script_membership() {
        assert_eq!(script_extensions('\u{0beb}'), [*b"Taml", *b"Gran"]);
    }

    #[test]
    fn enclosing_marks_make_the_whole_vertical_cluster_upright() {
        assert_eq!(
            vertical_glyph_orientation("A\u{20dd}"),
            VerticalGlyphOrientation::Upright
        );
        assert_eq!(
            vertical_glyph_orientation("A"),
            VerticalGlyphOrientation::Rotated
        );
    }
}

#[cfg(test)]
mod conformance_tests;
