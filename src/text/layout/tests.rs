use super::*;
use crate::text::test_support::{bundled_latin_font, windows_test_font};

#[test]
fn an_empty_text_line_is_the_additive_layout_identity() {
    let line = TextLine::default();

    assert!(line.runs.is_empty());
    assert_eq!(line.advance(), Vector2::ZERO);
}

#[test]
fn one_shaping_result_can_be_laid_out_at_multiple_sizes() {
    let font = bundled_latin_font();
    let shaped = GlyphRun::shape_with(font, "Theoria", &ShapingOptions::default()).unwrap();
    let small = LayoutRun::new(shaped.clone(), 16.0);
    let large = LayoutRun::new(shaped, 32.0);

    assert!(Arc::ptr_eq(&small.glyphs, &large.glyphs));
    assert!((large.advance().x - small.advance().x * 2.0).abs() < 0.001);
    assert!((large.advance().y - small.advance().y * 2.0).abs() < 0.001);
}

#[test]
fn font_size_validation_belongs_to_layout() {
    assert_eq!(validate_font_size(16.0), Ok(()));
    assert_eq!(validate_font_size(0.0), Ok(()));
    assert_eq!(
        validate_font_size(f32::NAN),
        Err(TextError::NonFiniteFontSize)
    );
    assert_eq!(validate_font_size(-1.0), Err(TextError::NegativeFontSize));
}

#[cfg(target_os = "windows")]
#[test]
fn a_text_line_places_multiple_fonts_by_accumulating_run_advances() {
    let Some(first_font) = windows_test_font() else {
        return;
    };
    let Ok(second_data) = std::fs::read(r"C:\Windows\Fonts\arial.ttf") else {
        return;
    };
    let second_font = Font::from_bytes(second_data).unwrap();
    let first = GlyphRun::shape_with(first_font, "Theoria ", &ShapingOptions::default()).unwrap();
    let second = GlyphRun::shape_with(second_font, "ffi", &ShapingOptions::default()).unwrap();
    let expected_second_origin = LayoutRun::new(first.clone(), 24.0).advance();
    let expected_advance = expected_second_origin + LayoutRun::new(second.clone(), 24.0).advance();

    let line = TextLine::from_runs(
        [
            InlineRun::unspaced(first, 24.0),
            InlineRun::unspaced(second, 24.0),
        ],
        "Theoria ffi",
    );

    assert_eq!(line.runs.len(), 2);
    assert_eq!(line.runs[0].inline_start, Vector2::ZERO.x);
    assert_eq!(line.runs[1].inline_start, expected_second_origin.x);
    assert_eq!(line.advance(), expected_advance);
}

#[cfg(target_os = "windows")]
#[test]
fn horizontal_runs_align_their_alphabetic_baselines() {
    let Some(font) = windows_test_font() else {
        return;
    };
    let font_size = 24.0;
    let text = Text::horizontal(TextStyle::new(&[font], font_size))
        .push("abc\u{0416}")
        .layout()
        .unwrap();

    for positioned in text.lines[0].line.runs.iter() {
        let run = &positioned.run;
        let expected = run
            .font()
            .baseline_coordinate(
                crate::font::BaselineAxis::Horizontal,
                run.script(),
                crate::font::BaselineKind::Alphabetic,
            )
            .unwrap_or(0.0)
            * run.font_size()
            / f32::from(run.font().metrics().units_per_em());
        assert!((positioned.placement.translation().y - expected).abs() < 0.001);
    }
}

#[cfg(target_os = "windows")]
#[test]
fn one_span_sequence_can_feed_both_writing_modes() {
    let Some(font) = windows_test_font() else {
        return;
    };
    let style = TextStyle::new(&[font], 16.0);
    let spans = vec![Span::new("shared"), Span::new(" spans")];

    let horizontal = Text::horizontal(style.clone())
        .extend(spans.iter().cloned())
        .layout()
        .unwrap();
    let vertical = Text::vertical(style).extend(spans).layout().unwrap();

    assert_eq!(horizontal.content(), "shared spans");
    assert_eq!(vertical.content(), horizontal.content());
}

#[test]
fn paragraph_ranges_remove_hard_breaks_and_preserve_empty_lines() {
    assert_eq!(
        line_breaking::paragraph_ranges("a\r\nb\n"),
        [0..1, 3..4, 5..5]
    );
    let empty = line_breaking::paragraph_ranges("");
    assert_eq!(empty.len(), 1);
    assert_eq!(empty[0], 0..0);
}

#[test]
fn bundled_font_drives_platform_independent_layout_inspection() {
    let font = bundled_latin_font();
    let text = Text::horizontal(TextStyle::new(&[font], 20.0))
        .push("first\nsecond")
        .layout()
        .unwrap();
    let lines: Vec<_> = text.lines().collect();

    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].source_range(), 0..5);
    assert!(lines[0].ascent() > 0.0);
    assert!(lines[0].descent() >= 0.0);
    assert!(lines[0].typographic_bounds().height() > 0.0);
    assert_eq!(text.paragraph_ranges().collect::<Vec<_>>(), [0..5, 6..12]);
    assert!(text.typographic_bounds().is_some());
}
