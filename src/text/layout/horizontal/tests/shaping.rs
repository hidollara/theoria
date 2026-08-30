use super::*;

#[test]
fn horizontal_text_shapes_each_typographic_span_and_keeps_global_offsets() {
    let font = bundled_latin_font();
    let default_style = TextStyle::new(std::slice::from_ref(&font), 16.0);
    let larger = default_style.clone().with_font_size(32.0);
    let text = Text::horizontal(default_style)
        .push("ab")
        .push(Span::new("cd").with_style(larger))
        .layout()
        .unwrap();

    let runs = &text.lines[0].line.runs;
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].run.source_range, 0..2);
    assert_eq!(runs[1].run.source_range, 2..4);
    assert_eq!(runs[0].run.font_size(), 16.0);
    assert_eq!(runs[1].run.font_size(), 32.0);
}
