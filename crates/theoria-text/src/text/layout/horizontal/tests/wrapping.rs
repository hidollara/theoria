use super::*;

#[test]
fn balanced_line_breaking_preserves_the_paragraph_and_bounded_lines() {
    let Some(font) = windows_test_font() else {
        return;
    };
    let fonts = [font];
    let width = horizontal(&fonts, 22.0, "alpha beta gamma")
        .layout()
        .unwrap()
        .width();
    let text = horizontal(
        &fonts,
        22.0,
        "alpha beta gamma delta epsilon zeta eta theta",
    )
    .wrap_width(width)
    .with_line_break_strategy(LineBreakStrategy::Balanced)
    .layout()
    .unwrap();

    assert!(text.line_count() > 1);
    assert_eq!(text.line_ranges().next().unwrap().start, 0);
    assert_eq!(text.line_ranges().last().unwrap().end, text.content().len());
    assert!(
        text.lines
            .iter()
            .all(|line| line.line.advance().x.abs() <= width + 0.001)
    );
}

#[test]
fn justification_can_cap_pathological_inter_word_expansion() {
    let Some(font) = windows_test_font() else {
        return;
    };
    let fonts = [font];
    let natural = horizontal(&fonts, 24.0, "a b").layout().unwrap();
    let target = natural.width() + 4.0;
    let text = horizontal(&fonts, 24.0, "a b c")
        .wrap_width(target)
        .aligned(TextAlignment::Justify)
        .with_justification_options(
            JustificationOptions::new().with_maximum_spacing_expansion(Some(2.0)),
        )
        .layout()
        .unwrap();
    assert!(text.line_count() > 1);
    assert!(text.lines[0].line.advance().x.abs() < target - 0.5);
}

#[test]
fn wrapped_text_keeps_its_layout_width_while_unwrapped_text_uses_natural_width() {
    let wrapped = horizontal(&[], 16.0, "")
        .wrap_width(120.0)
        .layout()
        .unwrap();
    let unwrapped = horizontal(&[], 16.0, "").layout().unwrap();

    assert_eq!(wrapped.width(), 120.0);
    assert_eq!(unwrapped.width(), 0.0);
}

#[cfg(target_os = "windows")]
#[test]
fn text_layout_wraps_at_the_last_fitting_unicode_opportunity() {
    let Some(font) = windows_test_font() else {
        return;
    };
    let fonts = [font];
    let first_line_width = horizontal(&fonts, 24.0, "one two ")
        .layout()
        .unwrap()
        .width();
    let text = horizontal(&fonts, 24.0, "one two three")
        .wrap_width(first_line_width + 0.001)
        .layout()
        .unwrap();

    assert_eq!(text.line_count(), 2);
    assert_eq!(text.line_ranges().collect::<Vec<_>>(), [0..8, 8..13]);
}

#[cfg(target_os = "windows")]
#[test]
fn soft_hyphen_is_invisible_without_a_break() {
    let Some(font) = windows_test_font() else {
        return;
    };
    let fonts = [font];
    let plain = horizontal(&fonts, 24.0, "cooperate").layout().unwrap();
    let discretionary = horizontal(&fonts, 24.0, "co\u{00ad}operate")
        .layout()
        .unwrap();

    assert_eq!(discretionary.content(), "co\u{00ad}operate");
    assert!((discretionary.width() - plain.width()).abs() < 0.001);
}

#[cfg(target_os = "windows")]
#[test]
fn wrapping_at_soft_hyphen_materializes_a_visible_hyphen() {
    let Some(font) = windows_test_font() else {
        return;
    };
    let fonts = [font];
    let content = "co\u{00ad}operate";
    let prefix = horizontal(&fonts, 24.0, "co").layout().unwrap();
    let hyphenated_prefix = horizontal(&fonts, 24.0, "co\u{2010}").layout().unwrap();
    let text = horizontal(&fonts, 24.0, content)
        .wrap_width(hyphenated_prefix.width() + 0.001)
        .layout()
        .unwrap();

    assert_eq!(text.content(), content);
    assert_eq!(text.line_count(), 2);
    assert_eq!(
        text.line_ranges().collect::<Vec<_>>(),
        [0.."co\u{00ad}".len(), "co\u{00ad}".len()..content.len()]
    );
    assert!(text.lines[0].line.advance().x > prefix.width());
    assert!((text.lines[0].line.advance().x - hyphenated_prefix.width()).abs() < 0.001);
}

#[cfg(target_os = "windows")]
#[test]
fn automatic_hyphenation_is_opt_in_and_preserves_logical_text() {
    let Some(font) = windows_test_font() else {
        return;
    };
    let fonts = [font];
    let content = "extensive";
    let hyphenated_prefix = horizontal(&fonts, 24.0, "ex\u{2010}").layout().unwrap();
    let width = hyphenated_prefix.width() + 0.001;
    let style = TextStyle::new(&fonts, 24.0).with_language("en-US");

    let manual = Text::horizontal(style.clone())
        .push(content)
        .wrap_width(width)
        .layout()
        .unwrap();
    let automatic = Text::horizontal(style)
        .push(content)
        .wrap_width(width)
        .with_hyphenator(std::sync::Arc::new(TestHyphenator))
        .layout()
        .unwrap();

    assert_eq!(manual.line_count(), 1);
    assert!(automatic.line_count() > 1);
    assert_eq!(automatic.content(), content);
    assert_eq!(automatic.line_ranges().next(), Some(0..2));
    assert!((automatic.lines[0].line.advance().x - hyphenated_prefix.width()).abs() < 0.001);
}

#[cfg(target_os = "windows")]
#[test]
fn automatic_hyphenation_without_a_supported_language_falls_back_to_unicode_breaks() {
    let Some(font) = windows_test_font() else {
        return;
    };
    let fonts = [font];
    let text = horizontal(&fonts, 24.0, "extensive")
        .wrap_width(1.0)
        .with_hyphenator(TestHyphenator)
        .layout()
        .unwrap();

    assert_eq!(text.line_count(), 1);
}

#[cfg(target_os = "windows")]
#[test]
fn automatic_hyphenation_crosses_typographic_spans_with_the_same_language() {
    let Some(font) = windows_test_font() else {
        return;
    };
    let fonts = [font];
    let default_style = TextStyle::new(&fonts, 24.0).with_language("en");
    let alternate_style = TextStyle::new(&fonts, 24.0).with_language("en");
    let width = horizontal(&fonts, 24.0, "ex\u{2010}")
        .layout()
        .unwrap()
        .width()
        + 0.001;
    let text = Text::horizontal(default_style)
        .push(Span::new("e").with_style(alternate_style))
        .push("xtensive")
        .wrap_width(width)
        .with_hyphenator(TestHyphenator)
        .layout()
        .unwrap();

    assert_eq!(text.line_ranges().next(), Some(0..2));
}

#[cfg(target_os = "windows")]
#[test]
fn automatic_hyphenation_uses_the_layout_language_as_a_fallback() {
    let Some(font) = windows_test_font() else {
        return;
    };
    let fonts = [font];
    let width = horizontal(&fonts, 24.0, "ex\u{2010}")
        .layout()
        .unwrap()
        .width()
        + 0.001;
    let text = horizontal(&fonts, 24.0, "extensive")
        .with_shaping_options(ShapingOptions::new().language("en"))
        .wrap_width(width)
        .with_hyphenator(TestHyphenator)
        .layout()
        .unwrap();

    assert_eq!(text.line_ranges().next(), Some(0..2));
}

#[cfg(target_os = "windows")]
#[test]
fn text_layout_preserves_hard_empty_lines_and_overlong_words() {
    let Some(font) = windows_test_font() else {
        return;
    };
    let fonts = [font];
    let hard_breaks = horizontal(&fonts, 20.0, "a\n\nb").layout().unwrap();
    assert_eq!(hard_breaks.line_count(), 3);
    assert_eq!(
        hard_breaks.line_ranges().collect::<Vec<_>>(),
        [0..1, 2..2, 3..4]
    );

    let overlong = horizontal(&fonts, 20.0, "unbreakable")
        .wrap_width(1.0)
        .layout()
        .unwrap();
    assert_eq!(overlong.line_count(), 1);
    assert!(overlong.width() < overlong.lines[0].line.advance().x.abs());
}
