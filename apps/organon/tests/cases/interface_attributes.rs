use organon_theoria::{Provide, Type, attribute::Value};
use theoria_canvas::{Color, ColorField, Opacity, color::Srgb};
mod ui {
    organon_theoria::library!(crate::ui);
}
fn field() -> ColorField {
    Color::new(Srgb, [10_u8, 20, 30], Opacity::<u8>::ONE).into()
}
#[organon_theoria::implement]
impl MeasurementValue {
    #[output(name = "reading")]
    fn reading(n: f32) -> Result<f32, f32> {
        if n >= 0.0 { Ok(n) } else { Err(n) }
    }
}

#[test]
fn interface_views_preserve_errors_and_follow_reactive_updates() {
    let reactive = theoria_reactive::Reactive::new();
    let measure = |n: f32| -> ui::types::HasDisplay {
        ui::Measurement::builder()
            .n(n)
            .build(&reactive)
            .unwrap()
            .provide()
    };
    let good = measure(42.0);
    assert_eq!(
        good.value_fixed()
            .unwrap()
            .as_ref()
            .unwrap()
            .text_fixed()
            .unwrap(),
        "42"
    );
    let bad = measure(-1.0);
    let Err(reason) = bad.value_fixed().unwrap() else {
        panic!("expected a failure")
    };
    assert_eq!(
        reason.downcast::<organon_theoria::types::number>(),
        Some(-1.0)
    );
    let n = reactive.create_state(Err::<f32, String>("offline".into()));
    let caption = ui::Caption::builder()
        .n(n.value())
        .build(&reactive)
        .unwrap()
        .caption();
    let read = reactive.create_computed(move |cx| {
        caption
            .read(&organon_theoria::ReadContext::new(cx))
            .into_cloned()
    });
    assert_eq!(*read.snapshot().unwrap(), Err("offline".into()));
    n.set(Ok(12.0)).unwrap();
    assert_eq!(*read.snapshot().unwrap(), Ok("12".into()));
}
#[test]
fn default_reference_and_overridden_parser_support_forward_and_nested_reactive_values() {
    let reactive = theoria_reactive::Reactive::new();
    let value = ui::DefaultReference::builder().build(&reactive).unwrap();
    assert!(value.drawing_fixed().unwrap().is_ok());
    let shade = reactive.create_state(field());
    let value = ui::Painted::builder()
        .shade(shade.value())
        .build(&reactive)
        .unwrap();
    assert!(value.drawing_fixed().is_none());
    let drawing = value.drawing();
    let count = std::rc::Rc::new(std::cell::Cell::new(0));
    let seen = count.clone();
    let read = reactive.create_computed(move |cx| {
        seen.set(seen.get() + 1);
        drawing.read(&organon_theoria::ReadContext::new(cx)).is_ok()
    });
    assert!(*read.snapshot().unwrap());
    shade.set(field()).unwrap();
    assert!(*read.snapshot().unwrap());
    assert_eq!(count.get(), 2);
}
#[test]
fn interface_parser_is_defined_once_and_rejects_nonconforming_values() {
    type ColorSource = organon_theoria::types::ColorSource;
    let reactive = theoria_reactive::Reactive::new();
    let registry = organon_theoria::attribute_types();
    let mut scope = organon_theoria::attribute::Scope::new(registry);
    scope.declare_contract("paint", "ColorField").unwrap();
    scope.set("paint", organon_theoria::Source::from(Value::new(field())));
    for text in ["paint", "{paint}", "#ff0000"] {
        let parsed = ColorSource::interpret(text, &scope).unwrap();
        let bound = {
            let source = parsed.source;
            organon_theoria::Calculation::new(move |cx| source.read(cx).into_cloned())
                .bind(&reactive)
                .unwrap()
        };
        assert!(bound.as_fixed().is_some());
    }
    assert!(ColorSource::interpret("missing", &scope).is_err());
    scope.declare_contract("number", "number").unwrap();
    scope.set("number", organon_theoria::Source::from(Value::new(1_f32)));
    assert!(ColorSource::interpret("number", &scope).is_err());
    let contract: ColorSource = field().provide();
    assert!(contract.field_fixed().is_some());
}

#[test]
fn local_interfaces_use_the_same_reference_policy() {
    let reactive = theoria_reactive::Reactive::new();
    let value = ui::LocalExample::builder()
        .shade(field())
        .build(&reactive)
        .unwrap();
    assert!(value.field_fixed().is_some());
    let views = ui::Views::builder()
        .shade(field())
        .build(&reactive)
        .unwrap();
    assert!(views.field_fixed().is_some());
}
#[test]
fn preview_resolves_xml_interface_outputs() {
    let loaded =
        organon_cargo::load(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("organon.xml"))
            .unwrap();
    let mut registry = preview::registry();
    for (local, id) in &loaded.loaded.origins {
        let identity = organon_cargo::identity(&id.source, &id.name).unwrap();
        let _ = registry.alias(local, &identity);
    }
    let program = loaded.program;
    let outputs = registry
        .evaluate(
            &program,
            "Views",
            [("shade".into(), organon_app::evaluation::Value::new(field()))].into(),
        )
        .unwrap();
    assert!(
        outputs["field"]
            .as_ref()
            .unwrap()
            .get::<ColorField>()
            .is_ok()
    );

    for (name, inputs) in [
        (
            "Painted",
            [("shade".into(), organon_app::evaluation::Value::new(field()))].into(),
        ),
        ("DefaultReference", Default::default()),
    ] {
        let outputs = registry.evaluate(&program, name, inputs).unwrap();
        assert!(
            outputs["drawing"]
                .as_ref()
                .unwrap()
                .get::<theoria_canvas::Canvas>()
                .is_ok()
        );
    }
}

mod preview {
    organon_app::preview_library!("organon.xml");
}
