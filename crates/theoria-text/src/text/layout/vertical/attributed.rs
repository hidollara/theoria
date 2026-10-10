use std::{borrow::Cow, ops::Range};

use super::{AttributedText, VerticalOrientation};

impl AttributedText {
    /// Returns the paragraph text used only for Unicode bidi resolution.
    ///
    /// CSS Writing Modes defines `text-orientation: upright` as if every
    /// affected character had strong left-to-right directionality. The
    /// placeholders retain each scalar's UTF-8 width so the bidi result's byte
    /// ranges continue to index the original semantic text.
    pub(super) fn bidi_text(&self, paragraph: Range<usize>) -> Cow<'_, str> {
        if !self.style_ranges.iter().any(|style_range| {
            style_range.style.vertical_orientation() == VerticalOrientation::Upright
                && style_range.source_range.start < paragraph.end
                && paragraph.start < style_range.source_range.end
        }) {
            return Cow::Borrowed(&self.content[paragraph]);
        }

        let mut text = String::with_capacity(paragraph.len());
        let mut cursor = paragraph.start;
        for style_range in self.style_ranges_within(paragraph.clone()) {
            if cursor < style_range.source_range.start {
                text.push_str(&self.content[cursor..style_range.source_range.start]);
            }
            let source = &self.content[style_range.source_range.clone()];
            if style_range.style.vertical_orientation() == VerticalOrientation::Upright {
                text.extend(source.chars().map(strong_ltr_placeholder));
            } else {
                text.push_str(source);
            }
            cursor = style_range.source_range.end;
        }
        if cursor < paragraph.end {
            text.push_str(&self.content[cursor..paragraph.end]);
        }
        debug_assert_eq!(text.len(), paragraph.len());
        Cow::Owned(text)
    }
}

fn strong_ltr_placeholder(character: char) -> char {
    match character.len_utf8() {
        1 => 'A',
        2 => '\u{00c0}',
        3 => '\u{0905}',
        4 => '\u{10000}',
        _ => unreachable!("UTF-8 scalars occupy one to four bytes"),
    }
}
