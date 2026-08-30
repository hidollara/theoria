use super::*;

struct TestHyphenator;

impl Hyphenator for TestHyphenator {
    fn hyphenate(&self, word: &str, language: &str) -> Vec<HyphenationOpportunity> {
        if language.starts_with("en") && word.replace('\u{00ad}', "") == "extensive" {
            [2, 5]
                .into_iter()
                .map(|index| HyphenationOpportunity::new(index, HyphenationMark::Hyphen))
                .collect()
        } else {
            Vec::new()
        }
    }
}

fn allowed_boundaries(text: &str) -> Vec<usize> {
    opportunities(text, LineBreakStyle::Strict, None)
        .filter_map(|(index, opportunity)| {
            (opportunity == BreakOpportunity::Allowed).then_some(index)
        })
        .collect()
}

#[test]
fn unicode_controls_keep_their_standard_break_semantics() {
    assert_eq!(allowed_boundaries("a\u{200b}b"), ["a\u{200b}".len()]);
    assert!(allowed_boundaries("a\u{00a0}b").is_empty());
    assert!(allowed_boundaries("a\u{2060}b").is_empty());
}

#[test]
fn soft_hyphen_supplies_a_discretionary_boundary() {
    assert_eq!(allowed_boundaries("a\u{00ad}b"), ["a\u{00ad}".len()]);
}

#[test]
fn injected_hyphenation_receives_the_full_bcp47_language() {
    let boundaries = provider_boundaries(
        "extensive",
        "en-US",
        &TestHyphenator,
        HyphenationOptions::default(),
    )
    .into_iter()
    .map(|boundary| boundary.index)
    .collect::<Vec<_>>();
    assert_eq!(boundaries, [2, 5]);
    assert!(
        provider_boundaries(
            "日本語",
            "ja",
            &TestHyphenator,
            HyphenationOptions::default(),
        )
        .is_empty()
    );
}

#[test]
fn explicit_soft_hyphens_take_precedence_over_dictionary_breaks() {
    assert!(
        provider_boundaries(
            "ex\u{00ad}tensive",
            "en",
            &TestHyphenator,
            HyphenationOptions::default(),
        )
        .is_empty()
    );
}

#[test]
fn provider_boundaries_validate_indices_and_preserve_unmarked_breaks() {
    struct UntrustedHyphenator;
    impl Hyphenator for UntrustedHyphenator {
        fn hyphenate(&self, word: &str, _: &str) -> Vec<HyphenationOpportunity> {
            vec![
                HyphenationOpportunity::new(0, HyphenationMark::Hyphen),
                HyphenationOpportunity::new(1, HyphenationMark::None),
                HyphenationOpportunity::new(word.len(), HyphenationMark::Hyphen),
                HyphenationOpportunity::new(usize::MAX, HyphenationMark::Hyphen),
            ]
        }
    }

    assert_eq!(
        provider_boundaries(
            "ab",
            "en",
            &UntrustedHyphenator,
            HyphenationOptions::new()
                .with_minimum_prefix(1)
                .with_minimum_suffix(1),
        ),
        [HyphenationBoundary {
            index: 1,
            show_hyphen: false,
            automatic: true,
        }]
    );
}

#[test]
fn cjk_opening_and_closing_punctuation_respect_line_boundaries() {
    assert_eq!(allowed_boundaries("日本"), ["日".len()]);
    assert!(allowed_boundaries("（日").is_empty());
    assert!(allowed_boundaries("日）").is_empty());
}

#[test]
fn dictionary_hyphenation_respects_grapheme_prefix_and_suffix_limits() {
    let strict = HyphenationOptions::new()
        .with_minimum_prefix(3)
        .with_minimum_suffix(4);
    let boundaries = provider_boundaries("extensive", "en", &TestHyphenator, strict)
        .into_iter()
        .map(|boundary| boundary.index)
        .collect::<Vec<_>>();
    assert_eq!(boundaries, [5]);
}

#[test]
fn break_word_adds_emergency_grapheme_boundaries_without_replacing_normal_breaks() {
    let candidates = candidates(
        "one unbreakable",
        &[],
        LineBreakStyle::Normal,
        None,
        OverflowWrap::BreakWord,
    );
    assert!(
        candidates
            .iter()
            .any(|candidate| candidate.index == 4 && !candidate.emergency)
    );
    assert!(
        candidates
            .iter()
            .any(|candidate| candidate.index == 5 && candidate.emergency)
    );
}

#[test]
fn css_line_break_strictness_is_delegated_to_icu() {
    let strict = candidates(
        "大ぁ",
        &[],
        LineBreakStyle::Strict,
        Some("ja-JP"),
        OverflowWrap::Normal,
    );
    let normal = candidates(
        "大ぁ",
        &[],
        LineBreakStyle::Normal,
        Some("ja-JP"),
        OverflowWrap::Normal,
    );
    let loose = candidates(
        "大ぁ",
        &[],
        LineBreakStyle::Loose,
        Some("ja-JP"),
        OverflowWrap::Normal,
    );
    let boundary = "大".len();
    assert!(!strict.iter().any(|candidate| candidate.index == boundary));
    assert!(normal.iter().any(|candidate| candidate.index == boundary));
    assert!(loose.iter().any(|candidate| candidate.index == boundary));
}
