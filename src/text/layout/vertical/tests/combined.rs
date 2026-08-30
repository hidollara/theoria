use super::*;

#[cfg(target_os = "windows")]
#[test]
fn combined_upright_places_a_horizontal_sequence_in_one_em_cell() {
    let Some(fonts) = windows_vertical_fonts() else {
        return;
    };
    let font_size = 32.0;
    let default_style = TextStyle::new(&fonts, font_size);
    let combined_style = default_style
        .clone()
        .with_vertical_orientation(VerticalOrientation::Combined);
    let text = Text::vertical(default_style)
        .push(Span::new("12").with_style(combined_style))
        .layout()
        .unwrap();

    assert_eq!(text.lines[0].line.advance().y, font_size);
    assert!(
        text.lines[0]
            .line
            .runs
            .iter()
            .all(|run| run.orientation == RunOrientation::Combined)
    );
    assert_eq!(text.size().y, font_size);
    assert!(text.coverage().is_ok());
}

#[cfg(target_os = "windows")]
#[test]
fn combined_upright_compresses_long_text_without_expanding_short_text() {
    let Some(fonts) = windows_vertical_fonts() else {
        return;
    };
    let font_size = 32.0;
    let style =
        TextStyle::new(&fonts, font_size).with_vertical_orientation(VerticalOrientation::Combined);
    let short = Text::vertical(style.clone()).push("1").layout().unwrap();
    let long = Text::vertical(style).push("1234").layout().unwrap();
    let short_scale = short.lines[0].line.runs[0]
        .placement
        .transform_vector(Vector2::new(1.0, 0.0))
        .x;
    let long_scale = long.lines[0].line.runs[0]
        .placement
        .transform_vector(Vector2::new(1.0, 0.0))
        .x;

    assert_eq!(short_scale, 1.0);
    assert!(long_scale > 0.0 && long_scale < 1.0);
    assert_eq!(short.lines[0].line.advance().y, font_size);
    assert_eq!(long.lines[0].line.advance().y, font_size);
}

#[cfg(target_os = "windows")]
#[test]
fn combined_upright_remains_one_cell_across_font_fallback_runs() {
    let Some(fonts) = windows_vertical_fonts() else {
        return;
    };
    let style =
        TextStyle::new(&fonts, 32.0).with_vertical_orientation(VerticalOrientation::Combined);
    let text = Text::vertical(style).push("A本").layout().unwrap();

    assert_eq!(text.lines[0].line.advance().y, 32.0);
    assert!(
        text.lines[0]
            .line
            .runs
            .iter()
            .all(|run| run.orientation == RunOrientation::Combined)
    );
}

#[cfg(target_os = "windows")]
#[test]
fn adjacent_combined_spans_remain_distinct_em_cells() {
    let Some(fonts) = windows_vertical_fonts() else {
        return;
    };
    let style =
        TextStyle::new(&fonts, 32.0).with_vertical_orientation(VerticalOrientation::Combined);
    let text = Text::vertical(TextStyle::new(&fonts, 32.0))
        .push(Span::new("12").with_style(style.clone()))
        .push(Span::new("34").with_style(style))
        .layout()
        .unwrap();

    assert_eq!(text.lines[0].line.advance().y, 64.0);
    let starts: Vec<_> = text.lines[0]
        .line
        .runs
        .iter()
        .map(|run| run.inline_start)
        .collect();
    assert!(starts.contains(&0.0));
    assert!(starts.contains(&32.0));
}
