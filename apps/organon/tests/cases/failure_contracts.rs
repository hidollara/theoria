mod ui {
    organon_theoria::library!(crate::ui);
}
mod preview {
    organon_app::preview_library!("organon.xml");
}

#[organon_theoria::implement]
impl Fault {
    type Value = f32;
}

#[organon_theoria::implement]
impl Raise {
    #[output(name = "value")]
    fn value() -> Result<String, f32> {
        Err(1.0)
    }
}

#[organon_theoria::implement]
impl NativeFirst {
    #[output(name = "label")]
    fn label(input: Input<'_, Self>) -> String {
        match &*input.value() {
            Ok(v) => v.clone(),
            Err(e) if e.downcast::<ui::types::First>().is_some() => "first".into(),
            Err(_) => "second".into(),
        }
    }
}

#[organon_theoria::implement]
impl NativeSecond {
    #[output(name = "label")]
    fn label(input: Input<'_, Self>) -> String {
        match &*input.value() {
            Ok(v) => v.clone(),
            Err(e) if e.downcast::<ui::types::First>().is_some() => "first".into(),
            Err(_) => "second".into(),
        }
    }
}

#[test]
fn native_inputs_and_interface_outputs_use_the_same_failure_precedence() {
    let graph = theoria_reactive::Reactive::new();
    let built = ui::BoundaryExample::builder().build(&graph).unwrap();
    assert_eq!(built.native_first_fixed().unwrap(), "first");
    assert_eq!(built.native_second_fixed().unwrap(), "second");
    assert_eq!(built.first_output_fixed().unwrap(), "first");
    assert_eq!(built.second_output_fixed().unwrap(), "second");
    let loaded =
        organon_cargo::load(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("organon.xml"))
            .unwrap();
    let outputs = preview::registry()
        .evaluate(&loaded.program, "BoundaryExample", Default::default())
        .unwrap();
    for (port, expected) in [
        ("native_first", "first"),
        ("native_second", "second"),
        ("first_output", "first"),
        ("second_output", "second"),
    ] {
        assert_eq!(
            outputs[port].as_ref().unwrap().get::<String>().unwrap(),
            expected,
            "{port}"
        );
    }
}

#[test]
fn overlapping_failure_interfaces_follow_the_port_declaration_order() {
    let graph = theoria_reactive::Reactive::new();
    let built = ui::Example::builder().build(&graph).unwrap();
    assert_eq!(built.first_fixed().unwrap(), "first");
    assert_eq!(built.second_fixed().unwrap(), "second");

    let loaded =
        organon_cargo::load(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("organon.xml"))
            .unwrap();
    let outputs = preview::registry()
        .evaluate(&loaded.program, "Example", Default::default())
        .unwrap();
    for expected in ["first", "second"] {
        assert_eq!(
            outputs[expected].as_ref().unwrap().get::<String>().unwrap(),
            expected
        );
    }
}
