use super::*;

#[cfg(target_os = "windows")]
#[test]
fn style_segments_follow_bidi_visual_order_without_losing_logical_ranges() {
    let Some(font) = windows_test_font() else {
        return;
    };
    let regular = TextStyle::new(std::slice::from_ref(&font), 20.0);
    let emphasized = regular.clone().with_font_size(28.0);
    let text = Text::horizontal(regular)
        .push("ab ")
        .push(Span::new("\u{05d0}\u{05d1}").with_style(emphasized))
        .push(" cd")
        .layout()
        .unwrap();

    let runs = &text.lines[0].line.runs;
    assert!(runs.len() >= 3);
    assert!(
        runs.iter()
            .any(|run| run.run.direction == ShapingDirection::RightToLeft)
    );
    assert!(
        runs.iter()
            .all(|run| run.run.source_range.end <= text.content().len())
    );
}

#[test]
fn horizontal_text_rejects_invalid_wrap_widths() {
    let style = TextStyle::new(&[], 16.0);
    assert_eq!(
        Text::horizontal(style.clone())
            .wrap_width(f32::NAN)
            .layout()
            .unwrap_err(),
        TextError::NonFiniteMaxWidth
    );
    assert_eq!(
        Text::horizontal(style)
            .wrap_width(-1.0)
            .layout()
            .unwrap_err(),
        TextError::NegativeMaxWidth
    );
}

#[test]
fn horizontal_text_rejects_non_finite_letter_spacing() {
    let style = TextStyle::new(&[], 16.0).with_letter_spacing(f32::NAN);

    assert_eq!(
        Text::horizontal(style).layout().unwrap_err(),
        TextError::NonFiniteLetterSpacing
    );
}

#[test]
fn horizontal_text_rejects_non_finite_word_spacing() {
    let style = TextStyle::new(&[], 16.0).with_word_spacing(f32::INFINITY);

    assert!(matches!(
        Text::horizontal(style).push("text").layout(),
        Err(TextError::NonFiniteWordSpacing)
    ));
}

#[test]
fn horizontal_text_rejects_invalid_tab_sizes() {
    let style = TextStyle::new(&[], 16.0);

    assert!(matches!(
        Text::horizontal(style.clone())
            .with_tab_size(f32::NAN)
            .layout(),
        Err(TextError::NonFiniteTabSize)
    ));
    assert!(matches!(
        Text::horizontal(style).with_tab_size(-1.0).layout(),
        Err(TextError::NegativeTabSize)
    ));
}

#[test]
fn paragraph_indents_and_spacing_affect_line_boxes_without_clipping() {
    let Some(font) = windows_test_font() else {
        return;
    };
    let fonts = [font];
    let word = horizontal(&fonts, 24.0, "word").layout().unwrap().size().x;
    let text = horizontal(&fonts, 24.0, "word word\nword")
        .wrap_width(word + 12.0)
        .with_paragraph_style(
            ParagraphStyle::new()
                .with_first_line_indent(10.0)
                .with_spacing_before(7.0)
                .with_spacing_after(5.0),
        )
        .layout()
        .unwrap();

    assert!(text.line_count() >= 3);
    assert_eq!(text.lines[0].baseline.x, 10.0);
    assert_eq!(text.lines[1].baseline.x, 0.0);
    let ordinary_gap = text.lines[1].baseline.y - text.lines[0].baseline.y;
    let paragraph_gap = text.lines[2].baseline.y - text.lines[1].baseline.y;
    assert!(paragraph_gap >= ordinary_gap + 12.0 - 0.001);
}

#[test]
fn paragraph_style_overrides_are_resolved_by_logical_paragraph_index() {
    let Some(font) = windows_test_font() else {
        return;
    };
    let fonts = [font];
    let text = horizontal(&fonts, 24.0, "first\nsecond")
        .wrap_width(240.0)
        .with_paragraph_style(ParagraphStyle::new().with_first_line_indent(4.0))
        .with_paragraph_style_at(
            1,
            ParagraphStyle::new()
                .with_first_line_indent(30.0)
                .with_alignment(InlineAlignment::End),
        )
        .layout()
        .unwrap();

    assert_eq!(text.line_count(), 2);
    assert_eq!(text.lines[0].baseline.x, 4.0);
    assert!(text.lines[1].baseline.x > 30.0);
}

#[test]
fn break_word_is_opt_in_for_overlong_ui_labels() {
    let Some(font) = windows_test_font() else {
        return;
    };
    let fonts = [font];
    let unit = horizontal(&fonts, 24.0, "un").layout().unwrap().size().x;
    let normal = horizontal(&fonts, 24.0, "unbreakable")
        .wrap_width(unit)
        .layout()
        .unwrap();
    let broken = horizontal(&fonts, 24.0, "unbreakable")
        .wrap_width(unit)
        .with_overflow_wrap(OverflowWrap::BreakWord)
        .layout()
        .unwrap();
    assert_eq!(normal.line_count(), 1);
    assert!(broken.line_count() > 1);
}

#[test]
fn line_limit_with_ellipsis_retains_logical_content_and_fits_the_box() {
    let Some(font) = windows_test_font() else {
        return;
    };
    let fonts = [font];
    let width = horizontal(&fonts, 24.0, "alpha beta")
        .layout()
        .unwrap()
        .size()
        .x;
    let text = horizontal(&fonts, 24.0, "alpha beta gamma delta")
        .wrap_width(width)
        .with_maximum_lines(1)
        .with_overflow(TextOverflow::Ellipsis)
        .layout()
        .unwrap();
    assert_eq!(text.content(), "alpha beta gamma delta");
    assert_eq!(text.line_count(), 1);
    assert!(text.line_ranges().next().unwrap().end < text.content().len());
    assert!(text.lines[0].line.advance().x.abs() <= width + 0.001);
}

#[test]
fn baseline_shift_moves_only_the_styled_run_toward_ascent() {
    let Some(font) = windows_test_font() else {
        return;
    };
    let fonts = [font];
    let base = TextStyle::new(&fonts, 24.0);
    let raised = base.clone().with_baseline_shift(6.0);
    let text = Text::horizontal(base)
        .push("a")
        .push(Span::new("b").with_style(raised))
        .layout()
        .unwrap();
    assert!(
        text.lines[0].line.runs[1].placement.translation().y
            < text.lines[0].line.runs[0].placement.translation().y
    );
}
