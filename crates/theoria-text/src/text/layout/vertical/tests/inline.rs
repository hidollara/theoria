use super::*;

#[cfg(target_os = "windows")]
#[test]
fn vertical_letter_spacing_expands_the_inline_axis_without_a_trailing_gap() {
    let Some(fonts) = windows_vertical_fonts() else {
        return;
    };
    let natural = Text::vertical(TextStyle::new(&fonts, 32.0))
        .push("日本")
        .layout()
        .unwrap();
    let tracked = Text::vertical(TextStyle::new(&fonts, 32.0).with_letter_spacing(4.0))
        .push("日本")
        .layout()
        .unwrap();

    assert!((tracked.height() - natural.height() - 4.0).abs() < 0.001);
}

#[cfg(target_os = "windows")]
#[test]
fn vertical_word_spacing_expands_a_separator_on_the_inline_axis() {
    let Some(fonts) = windows_vertical_fonts() else {
        return;
    };
    let natural = Text::vertical(TextStyle::new(&fonts, 32.0))
        .push("日 本")
        .layout()
        .unwrap();
    let spaced = Text::vertical(TextStyle::new(&fonts, 32.0).with_word_spacing(4.0))
        .push("日 本")
        .layout()
        .unwrap();

    assert!((spaced.height() - natural.height() - 4.0).abs() < 0.001);
}

#[cfg(target_os = "windows")]
#[test]
fn vertical_tabs_advance_to_space_based_stops() {
    let Some(fonts) = windows_vertical_fonts() else {
        return;
    };
    let horizontal_space = Text::horizontal(TextStyle::new(&fonts, 32.0))
        .push(" ")
        .layout()
        .unwrap()
        .width();
    let text = Text::vertical(TextStyle::new(&fonts, 32.0))
        .with_tab_size(4.0)
        .push("\t本")
        .layout()
        .unwrap();
    let field_start = text.lines[0].line.runs[1].inline_start;

    assert!((field_start - horizontal_space * 4.0).abs() < 0.001);
}
