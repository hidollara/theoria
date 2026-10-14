use organon_app::{
    Element,
    evaluation::{Value, Values},
    preview::badge,
};
use theoria_canvas::{
    Canvas, CanvasNode, Color, ColorField, CoverageNode, Opacity,
    color::DisplayP3,
    geometry::Point,
    path::{FillRule, Path},
};
use theoria_interaction::interaction::HitRegion;

const SOURCE: &str = include_str!("../src/preview/badge.organon.xml");

fn samples(width: f32) -> Values {
    Values::from([
        ("width".into(), Value::Scalar(width)),
        (
            "fill".into(),
            Value::Color(Color::new(DisplayP3, [40_u8, 220, 160], Opacity::<u8>::ONE)),
        ),
    ])
}

fn drawn_path(canvas: &Canvas) -> &Path {
    let CanvasNode::Draw(draw) = canvas.node() else {
        panic!("expected Draw")
    };
    let CoverageNode::Fill { path, .. } = draw.coverage.node() else {
        panic!("expected Fill")
    };
    path
}

#[test]
fn preview_preserves_native_fields_and_reuses_named_rectangles() {
    use theoria_canvas::{Coverage, GradientStop, LinearGradient};
    let source = r#"<element name="Example" version="1">
      <input name="brush" type="ColorField"/>
      <input name="host_canvas" type="Canvas"/>
      <input name="width" type="f32"/>
      <let name="outline" export="true"><Rect x="2" y="3" width="{width}" height="10"/></let>
      <let name="drawing" export="true"><Draw source="{brush}"><Fill><ref name="outline"/></Fill></Draw></let>
      <let name="canvas" export="true"><Draw source="{host_canvas}"><All/></Draw></let>
    </element>"#;
    let color = Color::new(DisplayP3, [40_u8, 220, 160], Opacity::<u8>::ONE);
    let gradient = LinearGradient::new(
        Point::new(0.0, 0.0),
        Point::new(100.0, 0.0),
        [
            GradientStop::new(0.0, color.clone()).unwrap(),
            GradientStop::new(1.0, color.clone()).unwrap(),
        ],
        DisplayP3,
    )
    .unwrap();
    let canvas = Canvas::draw(color, Coverage::all());
    let mut inputs = Values::from([
        ("brush".into(), Value::ColorField(gradient.clone().into())),
        ("host_canvas".into(), Value::Canvas(canvas.clone())),
        ("width".into(), Value::Scalar(30.0)),
    ]);
    let element = Element::from_xml(source).unwrap();
    assert_eq!(
        Element::from_xml(&element.to_xml().unwrap()).unwrap(),
        element
    );
    let output = element.evaluate(&inputs).unwrap();
    let Value::Path(outline) = &output["outline"] else {
        panic!()
    };
    assert_eq!(
        outline.subpaths()[0].segments()[0].end(),
        Point::new(32.0, 3.0)
    );
    let Value::Canvas(drawing) = &output["drawing"] else {
        panic!()
    };
    assert_eq!(drawn_path(drawing), outline);
    let CanvasNode::Draw(draw) = drawing.node() else {
        panic!()
    };
    let ColorField::LinearGradient(actual) = &draw.source else {
        panic!()
    };
    assert_eq!(actual, &gradient);
    let Value::Canvas(drawing) = &output["canvas"] else {
        panic!()
    };
    let CanvasNode::Draw(draw) = drawing.node() else {
        panic!()
    };
    let ColorField::Canvas(actual) = &draw.source else {
        panic!()
    };
    assert!(std::ptr::eq(actual.node(), canvas.node()));
    assert!(matches!(draw.coverage.node(), CoverageNode::All));
    for invalid in [-1.0, f32::NAN, f32::INFINITY] {
        inputs.insert("width".into(), Value::Scalar(invalid));
        assert!(element.evaluate(&inputs).is_err());
    }
    inputs.insert("width".into(), Value::Scalar(30.0));
    for invalid in ["prefix {brush}", "{outline}", "{missing}"] {
        let invalid = Element::from_xml(
            &source.replace("source=\"{brush}\"", &format!("source=\"{invalid}\"")),
        )
        .unwrap();
        assert!(invalid.evaluate(&inputs).is_err());
    }
}

#[test]
fn compiled_and_interpreted_outputs_share_shape_and_native_color() {
    let element = Element::from_xml(SOURCE).unwrap();
    for width in [160.0, 360.0] {
        let inputs = samples(width);
        let Value::Color(fill) = &inputs["fill"] else {
            panic!()
        };
        let reactive = theoria_reactive::Reactive::new();
        let compiled = badge::Badge::builder()
            .width(width)
            .fill(fill.clone())
            .build(&reactive)
            .unwrap();
        let compiled_outline = compiled.outline_fixed().unwrap().as_ref().unwrap();
        let compiled_drawing = compiled.drawing_fixed().unwrap().as_ref().unwrap();
        let interpreted = element.evaluate(&inputs).unwrap();
        let Value::Path(path) = &interpreted["outline"] else {
            panic!()
        };
        let Value::Canvas(canvas) = &interpreted["drawing"] else {
            panic!()
        };
        assert_eq!(path, compiled_outline);
        assert_eq!(drawn_path(canvas), path);
        assert_eq!(drawn_path(compiled_drawing), path);
        for canvas in [canvas, compiled_drawing] {
            let CanvasNode::Draw(draw) = canvas.node() else {
                panic!()
            };
            let ColorField::Solid(actual) = &draw.source else {
                panic!()
            };
            assert_eq!(actual, fill);
        }
        // The application owns interaction: the exported native Path plugs
        // straight into Theoria, including the concave notch of this badge.
        let hit = HitRegion::fill(path.clone(), FillRule::NonZero);
        assert!(hit.contains(Point::new(90.0, 160.0), 0.01));
        assert!(!hit.contains(Point::new(40.0, 160.0), 0.01));
        assert_eq!(hit.contains(Point::new(200.0, 100.0), 0.01), width > 200.0);
    }
}

#[test]
fn edits_round_trip_and_re_evaluate_without_rust_compilation() {
    let mut element = Element::from_xml(SOURCE).unwrap();
    let organon_app::element::ValueExpr::Path(organon_format::PathExpr::Polygon(points)) =
        &mut element.definitions[0].value
    else {
        panic!()
    };
    points[0].x = "64".into();
    let saved = element.to_xml().unwrap();
    assert_eq!(Element::from_xml(&saved).unwrap(), element);
    let outputs = element.evaluate(&samples(360.0)).unwrap();
    let Value::Path(path) = &outputs["outline"] else {
        panic!()
    };
    assert_eq!(path.subpaths()[0].start(), Point::new(64.0, 48.0));
}

#[test]
fn an_element_can_export_only_a_host_supplied_path() {
    let element = Element::from_xml(r#"<element name="Example" version="1"><input name="host" type="Path"/><let name="shape" export="true"><ref name="host"/></let></element>"#).unwrap();
    let path = Path::polygon([
        Point::new(0.0, 0.0),
        Point::new(2.0, 0.0),
        Point::new(0.0, 2.0),
    ]);
    let outputs = element
        .evaluate(&Values::from([("host".into(), Value::Path(path.clone()))]))
        .unwrap();
    let Value::Path(actual) = &outputs["shape"] else {
        panic!()
    };
    assert_eq!(actual, &path);
    assert!(
        organon_app::preview::element_content(
            &element,
            &Values::from([("host".into(), Value::Path(path))]),
            "shape",
            480.0,
            256.0
        )
        .is_err()
    );
}

#[test]
fn invalid_inputs_and_unresolved_paths_are_diagnostic_errors() {
    let element = Element::from_xml(SOURCE).unwrap();
    let mut missing = samples(360.0);
    missing.remove("width");
    assert!(
        element
            .evaluate(&missing)
            .unwrap_err()
            .to_string()
            .contains("width")
    );
    let mut wrong = samples(360.0);
    wrong.insert("width".into(), Value::String("360".into()));
    assert!(element.evaluate(&wrong).is_err());
    for width in [f32::NAN, f32::INFINITY] {
        assert!(element.evaluate(&samples(width)).is_err());
        let Value::Color(fill) = &samples(1.0)["fill"] else {
            panic!()
        };
        assert!(
            badge::Badge::builder()
                .width(width)
                .fill(fill.clone())
                .build(&theoria_reactive::Reactive::new())
                .unwrap()
                .drawing_fixed()
                .unwrap()
                .is_err()
        );
    }
    let bad = Element::from_xml(&SOURCE.replace("<ref name=\"outline\"", "<ref name=\"missing\""))
        .unwrap();
    assert!(
        bad.evaluate(&samples(360.0))
            .unwrap_err()
            .to_string()
            .contains("missing")
    );
}

#[test]
fn existing_text_and_composition_preview_matches_compiled_builders() {
    use organon_app::preview;
    use theoria_canvas::color::Srgb;
    use theoria_text::font::Font;
    let fonts = vec![Font::from_bytes(font_test_data::AHEM).unwrap()];
    for width in [200.0_f32, 480.0] {
        let mut label = Element::from_xml(include_str!("../src/preview/label.organon.xml"))
            .unwrap()
            .evaluate(&Values::from([
                ("fonts".into(), Value::Fonts(fonts.clone())),
                (
                    "label".into(),
                    Value::String("Organon — Text from XML\nResize to see words wrap.".into()),
                ),
                ("width".into(), Value::Scalar((width - 136.0).max(0.0))),
                ("font_size".into(), Value::Scalar(22.0)),
                (
                    "ink".into(),
                    Value::Color(Color::new(Srgb, [230_u8, 240, 255], Opacity::<u8>::ONE)),
                ),
            ]))
            .unwrap();
        let mut interpreted = Element::from_xml(include_str!("../src/preview/panel.organon.xml"))
            .unwrap()
            .evaluate(&Values::from([
                ("width".into(), Value::Scalar((width - 64.0).max(0.0))),
                (
                    "fill".into(),
                    Value::Color(Color::new(Srgb, [73_u8, 220, 177], Opacity::<u8>::ONE)),
                ),
                ("mark".into(), label.remove("drawing").unwrap()),
            ]))
            .unwrap();
        let Value::Canvas(interpreted) = interpreted.remove("drawing").unwrap() else {
            panic!()
        };
        same_drawing(&interpreted, &preview::drawing(width, &fonts).unwrap());
    }
}

fn same_drawing(first: &Canvas, second: &Canvas) {
    match (first.node(), second.node()) {
        (CanvasNode::Draw(a), CanvasNode::Draw(b)) => {
            let (ColorField::Solid(a_color), ColorField::Solid(b_color)) = (&a.source, &b.source)
            else {
                panic!("expected solid color fields")
            };
            assert_eq!(a_color, b_color);
            match (a.coverage.node(), b.coverage.node()) {
                (
                    CoverageNode::Fill { path: a, rule: ar },
                    CoverageNode::Fill { path: b, rule: br },
                ) => {
                    assert_eq!(a, b);
                    assert_eq!(ar, br);
                }
                (CoverageNode::All, CoverageNode::All) => (),
                _ => panic!("different coverage"),
            }
        }
        (
            CanvasNode::Transform {
                content: a,
                transform: at,
            },
            CanvasNode::Transform {
                content: b,
                transform: bt,
            },
        ) => {
            assert_eq!(at, bt);
            same_drawing(a, b);
        }
        (
            CanvasNode::Composite {
                backdrop: ab,
                source: a,
                composition: ac,
            },
            CanvasNode::Composite {
                backdrop: bb,
                source: b,
                composition: bc,
            },
        ) => {
            assert_eq!(ac, bc);
            same_drawing(ab, bb);
            same_drawing(a, b);
        }
        _ => panic!("different Canvas operations"),
    }
}

#[test]
fn inline_preview_matches_native_text_with_nested_style_overrides() {
    use theoria_canvas::color::{DisplayP3, Srgb};
    use theoria_text::{
        font::Font,
        text::{Span, Text, TextStyle},
    };
    let fonts = vec![Font::from_bytes(font_test_data::AHEM).unwrap()];
    let ink = Color::new(DisplayP3, [0.2_f32, 0.8, 0.4], Opacity::<f32>::ONE);
    let base_ink = Color::new(Srgb, [0x33_u8, 0x66, 0x99], Opacity::<u8>::ONE);
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
    let source =
        include_str!("../../../crates/organon-compiler/tests/fixtures/xml/inline_text.organon.xml");
    let element = Element::from_xml(source).unwrap();
    let mut inputs = Values::from([
        ("fonts".into(), Value::Fonts(fonts.clone())),
        ("alternate".into(), Value::Fonts(fonts)),
        ("label".into(), Value::String(label.into())),
        ("size".into(), Value::Scalar(30.0)),
        ("ink".into(), Value::Color(ink)),
    ]);
    let Value::Canvas(actual) = element
        .evaluate(&inputs)
        .unwrap()
        .remove("drawing")
        .unwrap()
    else {
        panic!()
    };
    same_drawing(&actual, &expected);
    for size in [-1.0, f32::NAN, f32::INFINITY] {
        inputs.insert("size".into(), Value::Scalar(size));
        assert!(element.evaluate(&inputs).is_err());
    }
}
