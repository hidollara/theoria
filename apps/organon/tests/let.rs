use organon_app::{
    Element,
    evaluation::{Value, Values},
};
use theoria_canvas::{CanvasNode, Color, CoverageNode, Opacity, color::Srgb, geometry::Point};

const SOURCE: &str =
    include_str!("../../../crates/organon-compiler/tests/fixtures/xml/lets.organon.xml");

#[test]
fn fixed_evaluation_errors_do_not_hide_unrelated_bound_outputs() {
    let element = Element::from_xml(
        r#"<element name="Example" version="1">
      <input name="width" type="f32"/>
      <let name="width_value" export="true"><ref name="width"/></let>
      <let name="outline" export="true"><Rect x="0" y="0" width="{width}" height="10"/></let>
    </element>"#,
    )
    .unwrap();
    let inputs = organon_app::evaluation::Bindings::from([(
        "width".into(),
        organon_app::evaluation::Source::fixed(Ok(Value::Scalar(-1.0))),
    )]);
    let outputs = element
        .bind(&theoria_reactive::Reactive::new(), &inputs)
        .unwrap();
    assert!(outputs["outline"].as_fixed().unwrap().is_err());
    assert!(
        matches!(outputs["width_value"].as_fixed().unwrap(), Ok(Value::Scalar(v)) if *v == -1.0)
    );
}

#[test]
fn top_level_aliases_and_forward_references_share_one_value() {
    let color = Color::new(Srgb, [10_u8, 20, 30], Opacity::<u8>::ONE);
    let element = Element::from_xml(SOURCE).unwrap();
    assert_eq!(
        Element::from_xml(&element.to_xml().unwrap()).unwrap(),
        element
    );
    let output = element
        .evaluate(&Values::from([
            ("width".into(), Value::Scalar(40.0)),
            ("label".into(), Value::String("hello".into())),
            ("color".into(), Value::Color(color.clone())),
        ]))
        .unwrap();
    assert_eq!(output.len(), 8);
    assert!(!output.contains_key("reused"));
    let Value::Canvas(inner) = &output["drawing"] else {
        panic!()
    };
    for name in ["copy", "alias"] {
        let Value::Canvas(alias) = &output[name] else {
            panic!()
        };
        assert!(std::ptr::eq(inner.node(), alias.node()));
    }
    let Value::Canvas(moved) = &output["moved"] else {
        panic!()
    };
    let CanvasNode::Transform { content, .. } = moved.node() else {
        panic!()
    };
    assert!(std::ptr::eq(content.node(), inner.node()));
    let Value::Path(shape) = &output["shape"] else {
        panic!()
    };
    let CanvasNode::Draw(draw) = inner.node() else {
        panic!()
    };
    let CoverageNode::Fill { path, .. } = draw.coverage.node() else {
        panic!()
    };
    assert_eq!(path, shape);
    assert_eq!(
        path.subpaths()[0].segments()[0].end(),
        Point::new(40.0, 0.0)
    );
    assert!(matches!(&output["extent"], Value::Scalar(40.0)));
    assert!(matches!(&output["caption"], Value::String(value) if value == "hello"));
    assert!(matches!(&output["ink"], Value::Color(value) if value == &color));
}

#[test]
fn cycles_unknown_names_and_context_type_mismatches_fail_in_preview() {
    for source in [
        r#"<element name="Example" version="1"><let name="a" export="true"><ref name="missing"/></let></element>"#,
        r#"<element name="Example" version="1"><let name="a" export="true"><ref name="b"/></let><let name="b"><ref name="a"/></let></element>"#,
        r#"<element name="Example" version="1"><let name="a" export="true"><Translate x="0" y="0"><ref name="b"/></Translate></let><let name="b"><Rect x="0" y="0" width="1" height="1"/></let></element>"#,
        r##"<element name="Example" version="1"><let name="a" export="true"><Draw source="#ffffff"><Fill><ref name="b"/></Fill></Draw></let><let name="b"><Draw source="#ffffff"><All/></Draw></let></element>"##,
        r#"<element name="Example" version="1"><let name="a" export="true"><ref name="a"/></let></element>"#,
    ] {
        let element = Element::from_xml(source).unwrap();
        assert!(element.evaluate(&Values::new()).is_err(), "{source}");
    }
}

#[test]
fn bound_preview_preserves_computed_results_and_inlines_in_the_readers_context() {
    use organon_app::evaluation::Bindings;
    use organon_app::evaluation::Source;
    use std::{cell::Cell, rc::Rc};
    use theoria_reactive::Reactive;
    let xml =
        include_str!("../../../crates/organon-compiler/tests/fixtures/xml/reactive.organon.xml");
    let element = Element::from_xml(xml).unwrap();
    assert_eq!(
        Element::from_xml(&element.to_xml().unwrap()).unwrap(),
        element
    );
    let reactive = Reactive::new();
    let width = reactive.create_state(Ok(Value::Scalar(30.0)));
    let color = reactive.create_state(Ok(Value::Color(Color::new(
        Srgb,
        [20_u8, 30, 40],
        Opacity::<u8>::ONE,
    ))));
    let reads = Rc::new(Cell::new(0));
    let count = reads.clone();
    let input = Source::reactive(width.value());
    let input = Source::inline(move |cx| {
        count.set(count.get() + 1);
        input.read(cx).into_cloned()
    });
    let output = element
        .bind(
            &reactive,
            &Bindings::from([
                ("width".into(), input),
                ("color".into(), Source::reactive(color.value())),
            ]),
        )
        .unwrap();
    assert_eq!(reads.get(), 0);
    assert!(output["fixed"].is_fixed());
    let cached = output["cached"].clone();
    let ordinary = reactive.create_effect(move |cx| {
        let (Value::Canvas(a), Value::Canvas(b)) = (
            cached.read(cx).into_cloned().unwrap(),
            cached.read(cx).into_cloned().unwrap(),
        ) else {
            panic!()
        };
        assert!(std::ptr::eq(a.node(), b.node()));
    });
    ordinary.run().unwrap();
    assert_eq!(reads.get(), 1);
    color
        .set(Ok(Value::Color(Color::new(
            Srgb,
            [70_u8, 20, 10],
            Opacity::<u8>::ONE,
        ))))
        .unwrap();
    ordinary.run().unwrap();
    assert_eq!(reads.get(), 1);
    let inline = output["direct"].clone();
    let reader = reactive.create_effect(move |cx| {
        let (Value::Canvas(a), Value::Canvas(b)) = (
            inline.read(cx).into_cloned().unwrap(),
            inline.read(cx).into_cloned().unwrap(),
        ) else {
            panic!()
        };
        assert!(!std::ptr::eq(a.node(), b.node()));
    });
    reader.run().unwrap();
    assert_eq!(reads.get(), 3);
    let consumer = output["consumer"].clone();
    let reader2 = reactive.create_effect(move |cx| {
        consumer.read(cx).into_cloned().unwrap();
        consumer.read(cx).into_cloned().unwrap();
    });
    reader2.run().unwrap();
    assert_eq!(reads.get(), 4);
    width.set(Ok(Value::Scalar(50.0))).unwrap();
    assert!(ordinary.is_stale());
    assert!(reader.is_stale());
    assert!(reader2.is_stale());
    ordinary.run().unwrap();
    assert_eq!(reads.get(), 5);
}

#[test]
fn bound_preview_folds_fixed_values_and_recovers_from_invalid_reactive_inputs() {
    use organon_app::evaluation::Bindings;
    use organon_app::evaluation::Source;
    use theoria_reactive::Reactive;
    let element = Element::from_xml(include_str!(
        "../../../crates/organon-compiler/tests/fixtures/xml/reactive.organon.xml"
    ))
    .unwrap();
    let reactive = Reactive::new();
    let color = Value::Color(Color::new(Srgb, [20_u8, 30, 40], Opacity::<u8>::ONE));
    let fixed = Bindings::from([
        ("width".into(), Source::fixed(Ok(Value::Scalar(30.0)))),
        ("color".into(), Source::fixed(Ok(color.clone()))),
    ]);
    assert!(
        element
            .bind(&reactive, &fixed)
            .unwrap()
            .values()
            .all(Source::is_fixed)
    );
    let width = reactive.create_state(Ok(Value::Scalar(-1.0)));
    let output = element
        .bind(
            &reactive,
            &Bindings::from([
                ("width".into(), Source::reactive(width.value())),
                ("color".into(), Source::fixed(Ok(color))),
            ]),
        )
        .unwrap();
    let reader = reactive.create_computed(move |cx| {
        (
            output["cached"].read(cx).into_cloned(),
            output["direct"].read(cx).into_cloned(),
        )
    });
    assert!(reader.snapshot().unwrap().0.is_err());
    width.set(Ok(Value::String("wrong type".into()))).unwrap();
    assert!(
        reader
            .snapshot()
            .unwrap()
            .1
            .as_ref()
            .unwrap_err()
            .to_string()
            .contains("expected Scalar")
    );
    width.set(Ok(Value::Scalar(12.0))).unwrap();
    let result = reader.snapshot().unwrap();
    assert!(result.0.is_ok());
    assert!(result.1.is_ok());
}
