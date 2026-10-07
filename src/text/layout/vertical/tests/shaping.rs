use super::super::line::synthesized_sideways_cross_offset;
use super::*;
use crate::text::test_support::bundled_latin_font;

#[cfg(target_os = "windows")]
#[test]
fn vertical_text_shapes_upright_and_sideways_runs_into_one_text() {
    let Some(fonts) = windows_vertical_fonts() else {
        return;
    };
    let text = Text::vertical(TextStyle::new(&fonts, 32.0))
        .push("A本B")
        .layout()
        .unwrap();

    assert_eq!(text.writing_mode(), WritingMode::Vertical);
    assert_eq!(text.line_count(), 1);
    let runs = &text.lines[0].line.runs;
    assert_eq!(
        runs.iter().map(|run| run.orientation).collect::<Vec<_>>(),
        [
            RunOrientation::Sideways,
            RunOrientation::Upright,
            RunOrientation::Sideways,
        ]
    );
    for positioned in runs
        .iter()
        .filter(|run| run.orientation == RunOrientation::Sideways)
    {
        let run = &positioned.run;
        let roman = run
            .font()
            .baseline_coordinate(
                crate::font::BaselineAxis::Vertical,
                run.script(),
                crate::font::BaselineKind::Alphabetic,
            )
            .map(|coordinate| {
                coordinate * run.font_size() / f32::from(run.font().metrics().units_per_em())
            });
        let center = run
            .font()
            .baseline_coordinate(
                crate::font::BaselineAxis::Vertical,
                run.script(),
                crate::font::BaselineKind::IdeographicCenter,
            )
            .map(|coordinate| {
                coordinate * run.font_size() / f32::from(run.font().metrics().units_per_em())
            });
        let expected = match (roman, center) {
            (Some(roman), Some(center)) => roman - center,
            _ => synthesized_sideways_cross_offset(run),
        };
        assert!((positioned.placement.offset().x - expected).abs() < 0.001);
    }
    assert!(text.lines[0].line.advance().y > 0.0);
}

#[test]
fn synthesized_sideways_baseline_centers_the_typographic_em_box() {
    let font = bundled_latin_font();
    let run = GlyphRun::shape_with(
        font,
        "Theoria",
        &ShapingOptions::default().exact_direction(ShapingDirection::LeftToRight),
    )
    .unwrap();
    let run = LayoutRun::new(run, 32.0);
    let metrics = font_line_metrics(run.font(), run.font_size());
    let offset = synthesized_sideways_cross_offset(&run);

    let left = metrics.descent - offset;
    let right = metrics.ascent + offset;
    assert!((left - right).abs() < 0.001);
}
