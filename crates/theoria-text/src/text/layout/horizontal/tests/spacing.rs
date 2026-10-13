use super::*;

#[cfg(target_os = "windows")]
#[test]
fn letter_spacing_expands_cluster_boundaries_without_a_trailing_gap() {
    let Some(font) = windows_test_font() else {
        return;
    };
    let fonts = [font];
    let natural = horizontal(&fonts, 24.0, "abc").layout().unwrap();
    let tracked = Text::horizontal(TextStyle::new(&fonts, 24.0).with_letter_spacing(3.0))
        .push("abc")
        .layout()
        .unwrap();

    assert!((tracked.width() - natural.width() - 6.0).abs() < 0.001);
}

#[cfg(target_os = "windows")]
#[test]
fn word_spacing_expands_only_explicit_word_separators() {
    let Some(font) = windows_test_font() else {
        return;
    };
    let fonts = [font];
    let natural = horizontal(&fonts, 24.0, "a b").layout().unwrap();
    let spaced = Text::horizontal(TextStyle::new(&fonts, 24.0).with_word_spacing(4.0))
        .push("a b")
        .layout()
        .unwrap();
    let unseparated = Text::horizontal(TextStyle::new(&fonts, 24.0).with_word_spacing(4.0))
        .push("ab")
        .layout()
        .unwrap();
    let natural_unseparated = horizontal(&fonts, 24.0, "ab").layout().unwrap();

    assert!((spaced.width() - natural.width() - 4.0).abs() < 0.001);
    assert!((unseparated.width() - natural_unseparated.width()).abs() < 0.001);
}

#[cfg(target_os = "windows")]
#[test]
fn letter_and_word_spacing_are_independent_and_additive() {
    let Some(font) = windows_test_font() else {
        return;
    };
    let fonts = [font];
    let natural = horizontal(&fonts, 24.0, "a b").layout().unwrap();
    let spaced = Text::horizontal(
        TextStyle::new(&fonts, 24.0)
            .with_letter_spacing(2.0)
            .with_word_spacing(3.0),
    )
    .push("a b")
    .layout()
    .unwrap();

    assert!((spaced.width() - natural.width() - 7.0).abs() < 0.001);
}

#[cfg(target_os = "windows")]
#[test]
fn rtl_word_spacing_expands_the_separator_advance() {
    let Some(font) = windows_test_font() else {
        return;
    };
    let fonts = [font];
    let content = "\u{05d0} \u{05d1}";
    let natural = horizontal(&fonts, 24.0, content).layout().unwrap();
    let spaced = Text::horizontal(TextStyle::new(&fonts, 24.0).with_word_spacing(4.0))
        .push(content)
        .layout()
        .unwrap();

    assert!((spaced.width() - natural.width() - 4.0).abs() < 0.001);
}

#[cfg(target_os = "windows")]
#[test]
fn wrapping_accounts_for_word_spacing() {
    let Some(font) = windows_test_font() else {
        return;
    };
    let fonts = [font];
    let natural_width = horizontal(&fonts, 24.0, "ab cd").layout().unwrap().width();
    let natural = horizontal(&fonts, 24.0, "ab cd")
        .wrap_width(natural_width + 0.001)
        .layout()
        .unwrap();
    let spaced = Text::horizontal(TextStyle::new(&fonts, 24.0).with_word_spacing(4.0))
        .push("ab cd")
        .wrap_width(natural_width + 0.001)
        .layout()
        .unwrap();

    assert_eq!(natural.line_count(), 1);
    assert_eq!(spaced.line_count(), 2);
}

#[cfg(target_os = "windows")]
#[test]
fn justification_expands_wrapped_lines_but_not_the_final_line() {
    let Some(font) = windows_test_font() else {
        return;
    };
    let fonts = [font];
    let width = horizontal(&fonts, 24.0, "one two ")
        .layout()
        .unwrap()
        .width();
    let text = horizontal(&fonts, 24.0, "one two three")
        .wrap_width(width + 0.001)
        .aligned(TextAlignment::Justify)
        .layout()
        .unwrap();

    assert_eq!(text.line_count(), 2);
    assert!((text.lines[0].line.advance().x - (width + 0.001)).abs() < 0.001);
    assert!(text.lines[1].line.advance().x < width);
}

#[cfg(target_os = "windows")]
#[test]
fn cjk_justification_expands_inter_character_boundaries_across_spans() {
    let Some(fonts) = windows_vertical_fonts() else {
        return;
    };
    let font_size = 28.0;
    let natural_width = horizontal(&fonts, font_size, "日本")
        .layout()
        .unwrap()
        .width();
    let width = natural_width + font_size * 0.25;
    let alternate = TextStyle::new(&fonts, font_size);
    let text = Text::horizontal(TextStyle::new(&fonts, font_size))
        .push(Span::new("日").with_style(alternate))
        .push("本日本")
        .wrap_width(width)
        .inline_aligned(InlineAlignment::Justify)
        .layout()
        .unwrap();

    assert_eq!(text.line_count(), 2);
    assert!((text.lines[0].line.advance().x - width).abs() < 0.001);
}

#[cfg(target_os = "windows")]
#[test]
fn forced_break_lines_are_not_justified() {
    let Some(font) = windows_test_font() else {
        return;
    };
    let fonts = [font];
    let natural = horizontal(&fonts, 24.0, "one two").layout().unwrap();
    let text = horizontal(&fonts, 24.0, "one two\nthree")
        .wrap_width(natural.width() + 20.0)
        .aligned(TextAlignment::Justify)
        .layout()
        .unwrap();

    assert!((text.lines[0].line.advance().x - natural.width()).abs() < 0.001);
}

#[cfg(target_os = "windows")]
#[test]
fn rtl_justification_fills_the_physical_line_width() {
    let Some(font) = windows_test_font() else {
        return;
    };
    let fonts = [font];
    let first_part = "\u{05d0}\u{05d1} \u{05d2}\u{05d3} ";
    let content = "\u{05d0}\u{05d1} \u{05d2}\u{05d3} \u{05d4}\u{05d5}";
    let width = horizontal(&fonts, 24.0, first_part)
        .layout()
        .unwrap()
        .width()
        + 0.001;
    let text = horizontal(&fonts, 24.0, content)
        .wrap_width(width)
        .aligned(TextAlignment::Justify)
        .layout()
        .unwrap();

    assert_eq!(text.line_count(), 2);
    assert!((text.lines[0].line.advance().x.abs() - width).abs() < 0.001);
}

#[cfg(target_os = "windows")]
#[test]
fn arabic_justification_inserts_tatweels_without_changing_logical_text() {
    let Some(font) = windows_test_font() else {
        return;
    };
    let fonts = [font];
    let first_part = "\u{0645}\u{0631}\u{062d}\u{0628}\u{0627} ";
    let content = "\u{0645}\u{0631}\u{062d}\u{0628}\u{0627} \u{0628}\u{0627}\u{0644}\u{0639}\u{0627}\u{0644}\u{0645}";
    let natural = horizontal(&fonts, 24.0, first_part).layout().unwrap();
    let width = natural.width() + 20.0;
    let text = horizontal(&fonts, 24.0, content)
        .wrap_width(width)
        .inline_aligned(InlineAlignment::Justify)
        .layout()
        .unwrap();

    assert_eq!(text.line_count(), 2);
    assert_eq!(text.content(), content);
    assert!((text.lines[0].line.advance().x.abs() - width).abs() < 0.001);
    let natural_glyphs: usize = natural.lines[0]
        .line
        .runs
        .iter()
        .map(|run| run.run.glyphs.len())
        .sum();
    let justified_glyphs: usize = text.lines[0]
        .line
        .runs
        .iter()
        .map(|run| run.run.glyphs.len())
        .sum();
    assert!(justified_glyphs > natural_glyphs);
}

#[cfg(target_os = "windows")]
#[test]
fn tabs_advance_to_repeating_space_based_stops() {
    let Some(font) = windows_test_font() else {
        return;
    };
    let fonts = [font];
    let space_width = horizontal(&fonts, 24.0, " ").layout().unwrap().width();
    let text = horizontal(&fonts, 24.0, "\tX")
        .with_tab_size(4.0)
        .layout()
        .unwrap();
    let field_start = text.lines[0].line.runs[1].inline_start;

    assert!((field_start - space_width * 4.0).abs() < 0.001);
    assert_eq!(text.content(), "\tX");
    assert_eq!(text.lines[0].line.runs[0].run.source_range, 0..1);
}

#[cfg(target_os = "windows")]
#[test]
fn a_tab_uses_the_next_stop_after_existing_text() {
    let Some(font) = windows_test_font() else {
        return;
    };
    let fonts = [font];
    let space_width = horizontal(&fonts, 24.0, " ").layout().unwrap().width();
    let text = horizontal(&fonts, 24.0, "a\tb")
        .with_tab_size(4.0)
        .layout()
        .unwrap();
    let field_start = text.lines[0].line.runs[2].inline_start;

    assert!((field_start - space_width * 4.0).abs() < 0.001);
}

#[cfg(target_os = "windows")]
#[test]
fn explicit_tab_stops_align_complete_fields() {
    let Some(font) = windows_test_font() else {
        return;
    };
    let fonts = [font];
    let field_width = horizontal(&fonts, 24.0, "field").layout().unwrap().width();
    for (alignment, expected_start) in [
        (TabAlignment::Start, 120.0),
        (TabAlignment::End, 120.0 - field_width),
        (TabAlignment::Center, 120.0 - field_width * 0.5),
    ] {
        let stops = TabStops::new([TabStop::new(120.0, alignment).unwrap()]).unwrap();
        let text = horizontal(&fonts, 24.0, "\tfield")
            .with_tab_stops(stops)
            .layout()
            .unwrap();
        let field_start = text.lines[0].line.runs[1].inline_start;
        assert!((field_start - expected_start).abs() < 0.01);
    }
}

#[cfg(target_os = "windows")]
#[test]
fn decimal_tabs_place_the_requested_character_at_the_stop() {
    let Some(font) = windows_test_font() else {
        return;
    };
    let fonts = [font];
    let stops = TabStops::new([TabStop::new(140.0, TabAlignment::Decimal('.')).unwrap()]).unwrap();
    let text = horizontal(&fonts, 24.0, "\t12.34")
        .with_tab_stops(stops)
        .layout()
        .unwrap();
    let prefix_width = horizontal(&fonts, 24.0, "12").layout().unwrap().width();
    let field_start = text.lines[0].line.runs[1].inline_start;
    assert!((field_start + prefix_width - 140.0).abs() < 0.01);
}

#[cfg(target_os = "windows")]
#[test]
fn rtl_letter_spacing_expands_visual_advance() {
    let Some(font) = windows_test_font() else {
        return;
    };
    let fonts = [font];
    let natural = horizontal(&fonts, 24.0, "\u{05d0}\u{05d1}\u{05d2}")
        .layout()
        .unwrap();
    let tracked = Text::horizontal(TextStyle::new(&fonts, 24.0).with_letter_spacing(3.0))
        .push("\u{05d0}\u{05d1}\u{05d2}")
        .layout()
        .unwrap();

    assert!((tracked.width() - natural.width() - 6.0).abs() < 0.001);
}

#[cfg(target_os = "windows")]
#[test]
fn letter_spacing_does_not_break_arabic_cursive_words() {
    let Some(font) = windows_test_font() else {
        return;
    };
    let fonts = [font];
    let content = "\u{0645}\u{0631}\u{062d}\u{0628}\u{0627}";
    let natural = horizontal(&fonts, 24.0, content).layout().unwrap();
    let tracked = Text::horizontal(TextStyle::new(&fonts, 24.0).with_letter_spacing(4.0))
        .push(content)
        .layout()
        .unwrap();

    assert!((tracked.width() - natural.width()).abs() < 0.001);
}

#[cfg(target_os = "windows")]
#[test]
fn letter_spacing_still_expands_arabic_word_boundaries() {
    let Some(font) = windows_test_font() else {
        return;
    };
    let fonts = [font];
    let content =
        "\u{0645}\u{0631}\u{062d}\u{0628}\u{0627} \u{0645}\u{0631}\u{062d}\u{0628}\u{0627}";
    let natural = horizontal(&fonts, 24.0, content).layout().unwrap();
    let tracked = Text::horizontal(TextStyle::new(&fonts, 24.0).with_letter_spacing(4.0))
        .push(content)
        .layout()
        .unwrap();

    assert!((tracked.width() - natural.width() - 8.0).abs() < 0.001);
}

#[cfg(target_os = "windows")]
#[test]
fn wrapping_accounts_for_letter_spacing() {
    let Some(font) = windows_test_font() else {
        return;
    };
    let fonts = [font];
    let natural_width = horizontal(&fonts, 24.0, "ab cd").layout().unwrap().width();
    let natural = horizontal(&fonts, 24.0, "ab cd")
        .wrap_width(natural_width + 0.001)
        .layout()
        .unwrap();
    let tracked = Text::horizontal(TextStyle::new(&fonts, 24.0).with_letter_spacing(4.0))
        .push("ab cd")
        .wrap_width(natural_width + 0.001)
        .layout()
        .unwrap();

    assert_eq!(natural.line_count(), 1);
    assert_eq!(tracked.line_count(), 2);
}

#[cfg(target_os = "windows")]
#[test]
fn nonzero_letter_spacing_disables_optional_ligatures_by_default() {
    let Some(font) = windows_test_font() else {
        return;
    };
    let fonts = [font];
    let natural = horizontal(&fonts, 24.0, "ffi").layout().unwrap();
    let tracked = Text::horizontal(TextStyle::new(&fonts, 24.0).with_letter_spacing(2.0))
        .push("ffi")
        .layout()
        .unwrap();
    let natural_glyphs: usize = natural.lines[0]
        .line
        .runs
        .iter()
        .map(|run| run.run.glyphs.len())
        .sum();
    let tracked_glyphs: usize = tracked.lines[0]
        .line
        .runs
        .iter()
        .map(|run| run.run.glyphs.len())
        .sum();

    assert!(tracked_glyphs >= natural_glyphs);
    assert_eq!(tracked_glyphs, 3);
}
