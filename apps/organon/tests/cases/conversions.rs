use organon_theoria::attribute::Value;
use theoria_canvas::{Canvas, CanvasNode, Color, ColorField, Coverage, Opacity, color::Srgb};
mod ui {
    organon_theoria::library!(crate::ui);
}
fn sample_color() -> Color {
    Color::new(Srgb, [20_u8, 40, 60], Opacity::<u8>::ONE)
}
fn assert_converted(solid: &Canvas, nested: &Canvas) {
    let CanvasNode::Draw(solid) = solid.node() else {
        panic!("expected Draw")
    };
    let CanvasNode::Draw(nested) = nested.node() else {
        panic!("expected Draw")
    };
    assert!(matches!(solid.source, ColorField::Solid(_)));
    assert!(matches!(nested.source, ColorField::Canvas(_)));
}
#[test]
fn rust_conversions_are_shared_by_compiled_code_and_preview() {
    let color = sample_color();
    let canvas = Canvas::draw(color.clone(), Coverage::all());
    let reactive = theoria_reactive::Reactive::new();
    let compiled = ui::Converted::builder()
        .color(color.clone())
        .canvas(canvas.clone())
        .build(&reactive)
        .unwrap();
    assert_converted(
        compiled.solid_fixed().unwrap().as_ref().unwrap(),
        compiled.nested_fixed().unwrap().as_ref().unwrap(),
    );

    let loaded =
        organon_cargo::load(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("organon.xml"))
            .unwrap();
    let mut registry = preview::registry();
    for (local, id) in &loaded.loaded.origins {
        let identity = organon_cargo::identity(&id.file, &id.name).unwrap();
        let _ = registry.alias(local, &identity);
    }
    let program = loaded.program;

    let outputs = registry
        .evaluate(
            &program,
            "Converted",
            [
                ("color".into(), organon_app::evaluation::Value::new(color)),
                ("canvas".into(), organon_app::evaluation::Value::new(canvas)),
            ]
            .into(),
        )
        .unwrap();
    assert_converted(
        &outputs["solid"].as_ref().unwrap().get::<Canvas>().unwrap(),
        &outputs["nested"].as_ref().unwrap().get::<Canvas>().unwrap(),
    );

    let field = registry
        .types
        .values()
        .find(|t| (t.rust_type)() == std::any::TypeId::of::<ColorField>())
        .unwrap();
    assert!((field.resolve)(&Value::new(2.0_f32)).is_err());
    let exact = ColorField::from(sample_color());
    assert!(
        (field.resolve)(&Value::new(exact))
            .unwrap()
            .get::<ColorField>()
            .is_ok()
    );
    let scalar = registry
        .types
        .values()
        .find(|t| (t.rust_type)() == std::any::TypeId::of::<f32>())
        .unwrap();
    assert!((scalar.resolve)(&Value::new(String::from("2"))).is_err());
}

mod preview {
    organon_app::preview_library!("organon.xml");
}
