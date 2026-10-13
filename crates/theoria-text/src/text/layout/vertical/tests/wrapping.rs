use super::*;

#[test]
fn vertical_wrap_height_does_not_change_layout_width() {
    let style = TextStyle::new(&[], 16.0);
    let unwrapped = Text::vertical(style.clone()).layout().unwrap();
    let wrapped = Text::vertical(style).wrap_height(120.0).layout().unwrap();

    assert_eq!(wrapped.height(), 120.0);
    assert_eq!(unwrapped.height(), 0.0);
    assert_eq!(wrapped.width(), unwrapped.width());
    assert!(!wrapped.has_typographic_overflow());
}

#[cfg(target_os = "windows")]
#[test]
fn vertical_wrapping_places_columns_right_to_left() {
    let Some(fonts) = windows_vertical_fonts() else {
        return;
    };
    let one = Text::vertical(TextStyle::new(&fonts, 28.0))
        .push("日本")
        .layout()
        .unwrap();
    let text = Text::vertical(TextStyle::new(&fonts, 28.0))
        .push("日本 日本")
        .wrap_height(one.height() + 0.001)
        .layout()
        .unwrap();

    assert!(text.line_count() >= 2);
    assert!(
        text.lines
            .windows(2)
            .all(|columns| columns[0].baseline.x > columns[1].baseline.x)
    );
}

#[cfg(target_os = "windows")]
#[test]
fn vertical_soft_hyphen_is_visible_only_at_a_selected_break() {
    let Some(fonts) = windows_vertical_fonts() else {
        return;
    };
    let plain = Text::vertical(TextStyle::new(&fonts, 24.0))
        .push("cooperate")
        .layout()
        .unwrap();
    let unbroken = Text::vertical(TextStyle::new(&fonts, 24.0))
        .push("co\u{00ad}operate")
        .layout()
        .unwrap();
    let hyphenated_prefix = Text::vertical(TextStyle::new(&fonts, 24.0))
        .push("co\u{2010}")
        .layout()
        .unwrap();
    let content = "co\u{00ad}operate";
    let wrapped = Text::vertical(TextStyle::new(&fonts, 24.0))
        .push(content)
        .wrap_height(hyphenated_prefix.height() + 0.001)
        .layout()
        .unwrap();

    assert!((unbroken.height() - plain.height()).abs() < 0.001);
    assert_eq!(wrapped.line_ranges().next(), Some(0.."co\u{00ad}".len()));
    assert!((wrapped.lines[0].line.advance().y - hyphenated_prefix.height()).abs() < 0.001);
}

#[cfg(target_os = "windows")]
#[test]
fn injected_hyphenation_can_wrap_vertical_text_without_changing_content() {
    let Some(fonts) = windows_vertical_fonts() else {
        return;
    };
    let content = "extensive";
    let hyphenated_prefix = Text::vertical(TextStyle::new(&fonts, 24.0))
        .push("ex\u{2010}")
        .layout()
        .unwrap();
    let text = Text::vertical(TextStyle::new(&fonts, 24.0).with_language("en"))
        .push(content)
        .wrap_height(hyphenated_prefix.height() + 0.001)
        .with_hyphenator(TestHyphenator)
        .layout()
        .unwrap();

    assert_eq!(text.content(), content);
    assert_eq!(text.line_ranges().next(), Some(0..2));
    assert!((text.lines[0].line.advance().y - hyphenated_prefix.height()).abs() < 0.001);
}

#[cfg(target_os = "windows")]
#[test]
fn vertical_justification_expands_nonfinal_columns() {
    let Some(fonts) = windows_vertical_fonts() else {
        return;
    };
    let height = Text::vertical(TextStyle::new(&fonts, 28.0))
        .push("日 本 ")
        .layout()
        .unwrap()
        .height()
        + 0.001;
    let text = Text::vertical(TextStyle::new(&fonts, 28.0))
        .push("日 本 日本")
        .wrap_height(height)
        .inline_aligned(InlineAlignment::Justify)
        .layout()
        .unwrap();

    assert_eq!(text.line_count(), 2);
    assert!((text.lines[0].line.advance().y - height).abs() < 0.001);
    assert!(text.lines[1].line.advance().y < height);
}

#[cfg(target_os = "windows")]
#[test]
fn vertical_cjk_justification_does_not_require_word_spaces() {
    let Some(fonts) = windows_vertical_fonts() else {
        return;
    };
    let font_size = 28.0;
    let natural_height = Text::vertical(TextStyle::new(&fonts, font_size))
        .push("日本")
        .layout()
        .unwrap()
        .height();
    let height = natural_height + font_size * 0.25;
    let text = Text::vertical(TextStyle::new(&fonts, font_size))
        .push("日本日本")
        .wrap_height(height)
        .inline_aligned(InlineAlignment::Justify)
        .layout()
        .unwrap();

    assert_eq!(text.line_count(), 2);
    assert!((text.lines[0].line.advance().y - height).abs() < 0.001);
}

#[cfg(target_os = "windows")]
#[test]
fn vertical_forced_break_columns_are_not_justified() {
    let Some(fonts) = windows_vertical_fonts() else {
        return;
    };
    let natural = Text::vertical(TextStyle::new(&fonts, 28.0))
        .push("日 本")
        .layout()
        .unwrap();
    let text = Text::vertical(TextStyle::new(&fonts, 28.0))
        .push("日 本\n日本")
        .wrap_height(natural.height() + 20.0)
        .inline_aligned(InlineAlignment::Justify)
        .layout()
        .unwrap();

    assert!((text.lines[0].line.advance().y - natural.height()).abs() < 0.001);
}

#[cfg(target_os = "windows")]
#[test]
fn justification_does_not_expand_inside_combined_upright_cells() {
    let Some(fonts) = windows_vertical_fonts() else {
        return;
    };
    let font_size = 28.0;
    let combined =
        TextStyle::new(&fonts, font_size).with_vertical_orientation(VerticalOrientation::Combined);
    let text = Text::vertical(TextStyle::new(&fonts, font_size))
        .push(Span::new("1 2").with_style(combined))
        .push("日本")
        .wrap_height(40.0)
        .inline_aligned(InlineAlignment::Justify)
        .layout()
        .unwrap();

    assert!(text.line_count() > 1);
    assert!((text.lines[0].line.advance().y - font_size).abs() < 0.001);
    assert!(
        text.lines[0]
            .line
            .runs
            .iter()
            .all(|run| run.orientation == RunOrientation::Combined)
    );
}
