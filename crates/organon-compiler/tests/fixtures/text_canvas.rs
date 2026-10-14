use theoria_canvas::{
    Canvas, CanvasNode, Color, ColorField, ColorValue, CoverageNode, Opacity, color::DisplayP3,
    geometry::Point, transform::Projective2,
};
use theoria_text::font::Font;

mod label {
    include!("generated/label.rs");
}

mod literal {
    include!("generated/literal.rs");
}

mod interpolated {
    include!("generated/interpolated_text.rs");
}

mod inline_text {
    include!("generated/inline_text.rs");
}

#[test]
fn inline_content_inherits_styles_and_preserves_expanded_controls() {
    use theoria_text::text::{Span, Text, TextStyle};
    let fonts = vec![Font::from_bytes(font_test_data::AHEM).unwrap()];
    let ink = Color::new(DisplayP3, [0.2_f32, 0.8, 0.4], Opacity::<f32>::ONE);
    let base_ink = Color::new(
        theoria_canvas::color::Srgb,
        [0x33_u8, 0x66, 0x99],
        Opacity::<u8>::ONE,
    );
    // Braces and markup in a value are text, not another template or XML tree.
    let label = "{literal}<br/>\n\tZ";
    let style = TextStyle::new(&fonts, 20.0);
    let emphasis = style
        .clone()
        .with_font_size(30.0)
        .with_foreground(ink.clone());
    let expected = Text::horizontal(style)
        .push("AA")
        .push(Span::new("B").with_style(emphasis.clone()))
        .push(Span::new("C").with_style(emphasis.clone().with_font_size(10.0)))
        .push(Span::new("D").with_style(emphasis.clone().with_font_size(15.0)))
        .push(Span::new("E\n").with_style(emphasis.clone()))
        .push(Span::new(label).with_style(emphasis))
        .push("F\tG\nH")
        .wrap_width(1000.0)
        .layout()
        .unwrap()
        .to_canvas(base_ink)
        .unwrap()
        .unwrap();
    let inputs = inline_text::Inputs {
        fonts: fonts.clone(),
        alternate: fonts,
        label: label.into(),
        size: 30.0,
        ink,
    };
    let actual = inline_text::build(&inputs).unwrap().drawing;
    fn collect(canvas: &Canvas, transform: Projective2, result: &mut Vec<(Point, Color)>) {
        match canvas.node() {
            CanvasNode::Draw(draw) => {
                let ColorField::Solid(color) = &draw.source else {
                    panic!()
                };
                let CoverageNode::Fill { path, .. } = draw.coverage.node() else {
                    panic!()
                };
                for subpath in path.subpaths() {
                    for segment in subpath.segments() {
                        result.push((
                            transform.try_transform_point(segment.end()).unwrap(),
                            color.clone(),
                        ));
                    }
                }
            }
            CanvasNode::Transform {
                content,
                transform: child,
            } => collect(content, child.then(transform).unwrap(), result),
            CanvasNode::Composite {
                backdrop, source, ..
            } => {
                collect(backdrop, transform, result);
                collect(source, transform, result);
            }
        }
    }
    let mut actual_points = Vec::new();
    let mut expected_points = Vec::new();
    collect(&actual, Projective2::IDENTITY, &mut actual_points);
    collect(&expected, Projective2::IDENTITY, &mut expected_points);
    assert!(!actual_points.is_empty());
    assert_eq!(actual_points, expected_points);
    for size in [-1.0, f32::NAN, f32::INFINITY] {
        assert!(
            inline_text::build(&inline_text::Inputs {
                size,
                ..inputs.clone()
            })
            .is_err()
        );
    }
}

fn interpolated_inputs() -> interpolated::Inputs {
    interpolated::Inputs {
        fonts: vec![Font::from_bytes(font_test_data::AHEM).unwrap()],
        label: "{not_a_binding}<>&\"".into(),
        count: 2.0,
        width: "1000".into(),
        size: "20".into(),
        ink: "#33669980".into(),
    }
}

#[test]
fn interpolation_matches_literal_text_and_does_not_reinterpret_input_values() {
    let input = interpolated_inputs();
    let ink = Color::new(
        theoria_canvas::color::Srgb,
        [0x33_u8, 0x66, 0x99],
        Opacity::<u8>::from(0x80),
    );
    let expected = label::build(&label::Inputs {
        fonts: input.fonts.clone(),
        label: "A{{not_a_binding}<>&\"}2 $label".into(),
        width: 1000.0,
        font_size: 20.0,
        ink: ink.clone(),
    })
    .unwrap()
    .drawing;
    let actual = interpolated::build(&input).unwrap().drawing;
    let mut actual_points = Vec::new();
    let mut expected_points = Vec::new();
    points(&actual, Projective2::IDENTITY, &ink, &mut actual_points);
    points(&expected, Projective2::IDENTITY, &ink, &mut expected_points);
    assert!(!actual_points.is_empty());
    assert_eq!(actual_points, expected_points);
}

#[test]
fn invalid_expanded_text_attributes_return_errors() {
    for invalid in ["not a number", "-1", "NaN", "inf", "{width}"] {
        let mut input = interpolated_inputs();
        input.width = invalid.into();
        assert!(interpolated::build(&input).is_err(), "{invalid}");
        input.width = "100".into();
        input.size = invalid.into();
        assert!(interpolated::build(&input).is_err(), "{invalid}");
    }
    for invalid in ["red", "#12345", "#あいう", "#gggggg"] {
        let mut input = interpolated_inputs();
        input.ink = invalid.into();
        assert!(interpolated::build(&input).is_err(), "{invalid}");
    }
}

#[test]
fn literal_text_compiles_and_draws_without_soft_wrapping() {
    let canvas = literal::build(&literal::Inputs {
        fonts: vec![Font::from_bytes(font_test_data::AHEM).unwrap()],
    })
    .unwrap()
    .drawing;
    let ink = Color::new(
        theoria_canvas::color::Srgb,
        [0x33_u8, 0x66, 0x99],
        Opacity::<u8>::from(0x80),
    );
    let mut vertices = Vec::new();
    points(&canvas, Projective2::IDENTITY, &ink, &mut vertices);
    let right = vertices.iter().map(|p| p.x).fold(0.0_f32, f32::max);
    let bottom = vertices.iter().map(|p| p.y).fold(0.0_f32, f32::max);
    assert!((right - 240.0).abs() < 0.01, "right = {right}");
    assert!((bottom - 40.0).abs() < 0.01);
}

fn inputs(text: &str, width: f32) -> label::Inputs {
    label::Inputs {
        fonts: vec![Font::from_bytes(font_test_data::AHEM).unwrap()],
        label: text.into(),
        width,
        font_size: 20.0,
        ink: Color::new(DisplayP3, [0.2_f32, 0.8, 0.4], Opacity::<f32>::ONE),
    }
}

// Ahem draws rectangular glyphs, so their path endpoints give exact ink bounds.
// Visiting the final tree also checks that wrapping did not introduce a clip.
fn points(canvas: &Canvas, transform: Projective2, ink: &Color, result: &mut Vec<Point>) {
    match canvas.node() {
        CanvasNode::Draw(draw) => {
            let ColorField::Solid(color) = &draw.source else {
                panic!()
            };
            assert_eq!(color, ink);
            let CoverageNode::Fill { path, .. } = draw.coverage.node() else {
                panic!()
            };
            for subpath in path.subpaths() {
                for segment in subpath.segments() {
                    result.push(transform.try_transform_point(segment.end()).unwrap());
                }
            }
        }
        CanvasNode::Transform {
            content,
            transform: child,
        } => {
            points(content, child.then(transform).unwrap(), ink, result);
        }
        CanvasNode::Composite {
            backdrop, source, ..
        } => {
            points(backdrop, transform, ink, result);
            points(source, transform, ink, result);
        }
    }
}

fn bounds(inputs: &label::Inputs) -> (Point, Point) {
    let mut vertices = Vec::new();
    points(
        &label::build(inputs).unwrap().drawing,
        Projective2::IDENTITY,
        &inputs.ink,
        &mut vertices,
    );
    assert!(!vertices.is_empty());
    vertices
        .iter()
        .fold((vertices[0], vertices[0]), |(min, max), p| {
            (
                Point::new(min.x.min(p.x), min.y.min(p.y)),
                Point::new(max.x.max(p.x), max.y.max(p.y)),
            )
        })
}

#[test]
fn words_wrap_but_an_unbreakable_word_is_not_clipped() {
    let wide = bounds(&inputs("AA AA", 200.0));
    let narrow = bounds(&inputs("AA AA", 45.0));
    assert!(narrow.1.y - narrow.0.y > wide.1.y - wide.0.y);
    assert!(narrow.1.x <= 45.0);
    let overflow = bounds(&inputs("AAAA", 10.0));
    assert!(overflow.1.x > 10.0);
    assert!((overflow.1.x - overflow.0.x - 80.0).abs() < 0.01);
}

#[test]
fn hard_breaks_and_font_size_affect_the_generated_drawing() {
    let one_line = bounds(&inputs("AA", 200.0));
    let two_lines = bounds(&inputs("A\nA", 200.0));
    assert!(two_lines.1.y - two_lines.0.y > one_line.1.y - one_line.0.y);
    let mut larger = inputs("AA", 200.0);
    larger.font_size = 40.0;
    let doubled = bounds(&larger);
    assert!((doubled.1.x - doubled.0.x - 2.0 * (one_line.1.x - one_line.0.x)).abs() < 0.01);
}

#[test]
fn empty_and_whitespace_text_are_transparent_drawings() {
    for text in ["", " ", "\n\t"] {
        let canvas = label::build(&inputs(text, 100.0)).unwrap().drawing;
        let CanvasNode::Draw(draw) = canvas.node() else {
            panic!()
        };
        let ColorField::Solid(color) = &draw.source else {
            panic!()
        };
        assert_eq!(
            color.value(),
            &ColorValue::U8([0; 3], theoria_canvas::Alpha::U8(0))
        );
    }
}

#[test]
fn dynamic_invalid_text_values_return_errors() {
    for invalid in [-1.0, f32::NAN, f32::INFINITY] {
        let mut input = inputs("A", 100.0);
        input.width = invalid;
        assert!(label::build(&input).is_err());
        input.width = 100.0;
        input.font_size = invalid;
        assert!(label::build(&input).is_err());
    }
    let mut input = inputs("A", 0.0);
    assert!(label::build(&input).is_ok());
    input.font_size = 0.0;
    assert!(label::build(&input).is_ok());
    input.fonts.clear();
    assert!(label::build(&input).is_err());
}
