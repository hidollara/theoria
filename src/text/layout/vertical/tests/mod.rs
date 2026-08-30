use super::*;
use crate::text::test_support::windows_vertical_fonts;
use crate::text::{HyphenationMark, HyphenationOpportunity, Hyphenator, Span};

#[derive(Clone, Copy)]
struct TestHyphenator;

impl Hyphenator for TestHyphenator {
    fn hyphenate(&self, word: &str, language: &str) -> Vec<HyphenationOpportunity> {
        if language.starts_with("en") && word == "extensive" {
            [2, 5]
                .into_iter()
                .map(|index| HyphenationOpportunity::new(index, HyphenationMark::Hyphen))
                .collect()
        } else {
            Vec::new()
        }
    }
}

mod combined;
mod inline;
mod placement;
mod shaping;
mod wrapping;
