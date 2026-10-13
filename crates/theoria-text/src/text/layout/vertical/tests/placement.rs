use super::*;

#[cfg(target_os = "windows")]
#[test]
fn vertical_alignment_positions_each_column_inside_the_layout_height() {
    let Some(fonts) = windows_vertical_fonts() else {
        return;
    };
    let style = TextStyle::new(&fonts, 24.0);
    let top = Text::vertical(style.clone())
        .push("日本")
        .wrap_height(200.0)
        .vertically_aligned(TextVerticalAlignment::Top)
        .layout()
        .unwrap();
    let center = Text::vertical(style.clone())
        .push("日本")
        .wrap_height(200.0)
        .vertically_aligned(TextVerticalAlignment::Center)
        .layout()
        .unwrap();
    let bottom = Text::vertical(style)
        .push("日本")
        .wrap_height(200.0)
        .vertically_aligned(TextVerticalAlignment::Bottom)
        .layout()
        .unwrap();

    assert!(top.lines[0].baseline.y < center.lines[0].baseline.y);
    assert!(center.lines[0].baseline.y < bottom.lines[0].baseline.y);
}

#[cfg(target_os = "windows")]
#[test]
fn fixed_column_width_controls_vertical_baseline_distance() {
    let Some(fonts) = windows_vertical_fonts() else {
        return;
    };
    let text = Text::vertical(TextStyle::new(&fonts, 24.0))
        .push("A\nB")
        .with_column_width(40.0)
        .layout()
        .unwrap();

    assert_eq!(text.line_count(), 2);
    assert_eq!(text.lines[0].baseline.x - text.lines[1].baseline.x, 40.0);
}

#[test]
fn vertical_column_width_is_validated_without_panicking() {
    let style = TextStyle::new(&[], 16.0);
    assert_eq!(
        Text::vertical(style.clone())
            .with_column_width(f32::NAN)
            .layout()
            .unwrap_err(),
        TextError::NonFiniteColumnWidth
    );
    assert_eq!(
        Text::vertical(style)
            .with_column_width(-1.0)
            .layout()
            .unwrap_err(),
        TextError::NegativeColumnWidth
    );
}

#[test]
fn vertical_columns_can_progress_left_to_right_explicitly() {
    let Some(fonts) = windows_vertical_fonts() else {
        return;
    };
    let one_column = Text::vertical(TextStyle::new(&fonts, 28.0))
        .push("日本")
        .layout()
        .unwrap()
        .height();
    let rtl = Text::vertical(TextStyle::new(&fonts, 28.0))
        .push("日本日本")
        .wrap_height(one_column + 0.001)
        .layout()
        .unwrap();
    let ltr = Text::vertical(TextStyle::new(&fonts, 28.0))
        .push("日本日本")
        .wrap_height(one_column + 0.001)
        .with_column_direction(VerticalColumnDirection::LeftToRight)
        .layout()
        .unwrap();
    assert!(rtl.lines[1].baseline.x < rtl.lines[0].baseline.x);
    assert!(ltr.lines[1].baseline.x > ltr.lines[0].baseline.x);
}

#[test]
fn vertical_paragraph_geometry_uses_inline_and_block_axes() {
    let Some(fonts) = windows_vertical_fonts() else {
        return;
    };
    let text = Text::vertical(TextStyle::new(&fonts, 24.0))
        .push("本文\n次")
        .wrap_height(160.0)
        .with_paragraph_style(ParagraphStyle::new().with_first_line_indent(8.0))
        .with_paragraph_style_at(
            1,
            ParagraphStyle::new()
                .with_first_line_indent(24.0)
                .with_spacing_before(12.0),
        )
        .layout()
        .unwrap();

    assert_eq!(text.line_count(), 2);
    assert_eq!(text.lines[0].baseline.y, 8.0);
    assert_eq!(text.lines[1].baseline.y, 24.0);
    assert!(text.lines[0].baseline.x - text.lines[1].baseline.x > 12.0);
}
