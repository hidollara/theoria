mod ui {
    organon_theoria::library!(crate::ui);
}
mod preview {
    organon_app::preview_library!("organon.xml");
}

#[test]
fn interface_failures_are_recovered_without_downcasting_the_original_type() {
    use organon_theoria::{Source, flow, types};
    let graph = theoria_reactive::Reactive::new();
    let display = <f32 as organon_theoria::Provide<organon_theoria::Display>>::provide(&-1.0);
    let text = graph.create_state(Err::<String, _>(display.clone()));
    let built = ui::InterfaceFailure::builder()
        .text(text.value())
        .build(&graph)
        .unwrap();
    // An unreachable concrete catch must not add String to the output's errors.
    let label: Source<String> = built.label();
    let read = graph.create_computed(move |cx| {
        label
            .read(&organon_theoria::ReadContext::new(cx))
            .into_cloned()
    });
    assert_eq!(*read.snapshot().unwrap(), "offline");
    text.set(Ok("online".into())).unwrap();
    assert_eq!(*read.snapshot().unwrap(), "online");
    text.set(Err(display.clone())).unwrap();
    assert_eq!(*read.snapshot().unwrap(), "offline");

    let loaded =
        organon_cargo::load(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("organon.xml"))
            .unwrap();
    let registry = preview::registry();
    for (input, expected) in [
        (
            Err(flow::Error::Failure(flow::Failure::new::<types::Display>(
                display,
            ))),
            "offline",
        ),
        (
            Ok(organon_app::evaluation::Value::new("online".to_owned())),
            "online",
        ),
    ] {
        let outputs = registry
            .evaluate_with_inputs(
                &loaded.program,
                "InterfaceFailure",
                [("text".into(), input)].into(),
            )
            .unwrap();
        assert_eq!(
            outputs["label"].as_ref().unwrap().get::<String>().unwrap(),
            expected
        );
    }
}
#[organon_theoria::implement]
impl IsReady {
    fn test(text: &String) -> bool {
        text == "ready"
    }
}

#[test]
fn xml_predicate_inputs_recover_declared_reasons_and_propagate_other_failures() {
    use organon_theoria::{DeclaredFailure, Source, flow, types};
    let graph = theoria_reactive::Reactive::new();
    let loaded =
        organon_cargo::load(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("organon.xml"))
            .unwrap();
    let registry = preview::registry();
    for (value, local, outer) in [
        (Ok("ready".to_owned()), Ok("yes"), "yes"),
        (Ok("pending".to_owned()), Ok("no"), "no"),
        (
            Err(DeclaredFailure::new::<types::number, _>(-1.0)),
            Ok("yes"),
            "caller",
        ),
        (
            Err(DeclaredFailure::new::<types::String, _>("offline".into())),
            Err("offline"),
            "caller",
        ),
    ] {
        let input = value
            .clone()
            .map(organon_app::evaluation::Value::new)
            .map_err(|e| flow::Error::Failure(e.into_failure()));
        let built = ui::PredicateInputs::builder()
            .sample(value)
            .build(&graph)
            .unwrap();
        let recovered: Source<Result<String, String>> = built.local();
        assert_eq!(
            recovered
                .as_fixed()
                .unwrap()
                .as_ref()
                .map(String::as_str)
                .map_err(String::as_str),
            local
        );
        assert_eq!(built.outer_fixed().unwrap(), outer);
        let result = registry
            .evaluate_with_inputs(
                &loaded.program,
                "PredicateInputs",
                [("sample".into(), input)].into(),
            )
            .unwrap();
        let label = result["local"]
            .as_ref()
            .map(|v| v.get::<String>().unwrap())
            .map_err(|e| {
                let flow::Error::Failure(f) = e else {
                    panic!("{e}")
                };
                f.downcast::<types::String>().unwrap()
            });
        assert_eq!(
            label.as_ref().map(String::as_str).map_err(String::as_str),
            local
        );
        assert_eq!(
            result["outer"].as_ref().unwrap().get::<String>().unwrap(),
            outer
        );
    }
}
#[organon_theoria::implement]
impl ThirdNumber {
    type Value = f32;
}
#[organon_theoria::implement]
impl TwoErrors {
    #[output(name = "label")]
    fn label(n: f32) -> Result<String, _> {
        use organon_theoria::{DeclaredFailure, types};
        if n < 0.0 {
            Err(DeclaredFailure::new::<types::number, _>(n))
        } else {
            Err(DeclaredFailure::new::<types::String, _>("text".into()))
        }
    }
}
#[organon_theoria::implement]
impl TwoPredicateErrors {
    fn test() -> Result<bool, _> {
        use organon_theoria::{DeclaredFailure, types};
        Err(DeclaredFailure::new::<types::number, _>(1.0))
    }
}
#[organon_theoria::implement]
impl AcceptErrors {
    #[output(name = "label")]
    fn label(input: Input<'_, Self>) -> String {
        match &*input.value() {
            Ok(s) => s.clone(),
            Err(e) => e
                .downcast::<organon_theoria::types::number>()
                .unwrap()
                .to_string(),
        }
    }
}
#[test]
fn declared_native_errors_cross_both_execution_paths() {
    let graph = theoria_reactive::Reactive::new();
    let loaded =
        organon_cargo::load(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("organon.xml"))
            .unwrap();
    let registry = preview::registry();
    assert_eq!(
        ui::ForwardErrors::builder()
            .build(&graph)
            .unwrap()
            .label_fixed()
            .unwrap(),
        "-8"
    );
    let forwarded = registry
        .evaluate(&loaded.program, "ForwardErrors", Default::default())
        .unwrap();
    assert_eq!(
        forwarded["label"]
            .as_ref()
            .unwrap()
            .get::<String>()
            .unwrap(),
        "-8"
    );
    for (n, expected) in [(-2.0_f32, "number -2"), (1.0, "text")] {
        let built = ui::ReadNativeErrors::builder().n(n).build(&graph).unwrap();
        assert_eq!(built.label_fixed().unwrap(), expected);
        let inputs = [("n".into(), organon_app::evaluation::Value::new(n))].into();
        let outputs = registry
            .evaluate(&loaded.program, "ReadNativeErrors", inputs)
            .unwrap();
        assert_eq!(
            outputs["label"].as_ref().unwrap().get::<String>().unwrap(),
            expected
        );
        let predicate = built.predicate();
        let result = graph.create_computed(move |cx| {
            predicate
                .read(&organon_theoria::ReadContext::new(cx))
                .into_cloned()
        });
        assert_eq!(*result.snapshot().unwrap(), "caught");
        assert_eq!(
            outputs["predicate"]
                .as_ref()
                .unwrap()
                .get::<String>()
                .unwrap(),
            "caught"
        );
    }
}
#[test]
fn preview_uses_the_same_handler_and_preserves_its_payload() {
    let loaded =
        organon_cargo::load(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("organon.xml"))
            .unwrap();
    let registry = preview::registry();
    for (number, ok) in [(1.0_f32, true), (-1.0, false)] {
        let mut outputs = registry
            .evaluate_typed(
                &loaded.program,
                "Control",
                [("number".into(), organon_app::evaluation::Value::new(number))].into(),
            )
            .unwrap();
        match outputs.remove("label").unwrap() {
            Ok(v) => {
                assert!(ok);
                assert_eq!(v.get::<String>().unwrap(), "ready");
            }
            Err(organon_theoria::flow::Error::Failure(e)) => {
                assert!(!ok);
                assert_eq!(e.value.get::<String>().unwrap(), "negative");
            }
            Err(e) => panic!("{e}"),
        }
    }
}
#[organon_theoria::implement]
impl Broken {
    fn test() -> Result<bool, f32> {
        Err(12.0)
    }
}
#[test]
fn predicate_failures_can_be_caught_by_interface_and_conditions_short_circuit() {
    let graph = theoria_reactive::Reactive::new();
    assert_eq!(
        ui::CatchDisplay::builder()
            .build(&graph)
            .unwrap()
            .label_fixed()
            .unwrap(),
        "12"
    );
    assert_eq!(
        ui::ShortCircuit::builder()
            .build(&graph)
            .unwrap()
            .label_fixed()
            .unwrap(),
        "first"
    );
    let loaded =
        organon_cargo::load(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("organon.xml"))
            .unwrap();
    let registry = preview::registry();
    for (name, label) in [("CatchDisplay", "12"), ("ShortCircuit", "first")] {
        let outputs = registry
            .evaluate(&loaded.program, name, Default::default())
            .unwrap();
        assert_eq!(
            outputs["label"].as_ref().unwrap().get::<String>().unwrap(),
            label
        );
    }
}
#[organon_theoria::implement]
impl Risk {
    #[output(name = "checked")]
    fn checked(input: Input<'_, Self>) -> Result<String, f32> {
        let n = *input.value();
        if n < 0.0 { Err(n) } else { Ok("ok".into()) }
    }
}
#[test]
fn native_failures_multiple_reasons_and_no_match() {
    let graph = theoria_reactive::Reactive::new();
    let loaded =
        organon_cargo::load(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("organon.xml"))
            .unwrap();
    let registry = preview::registry();
    for (n, expected) in [(-3.0_f32, "recovered -3"), (1.0, "ok")] {
        assert_eq!(
            ui::Recover::builder()
                .number(n)
                .build(&graph)
                .unwrap()
                .label_fixed()
                .unwrap(),
            expected
        );
        let inputs = [("number".into(), organon_app::evaluation::Value::new(n))].into();
        let outputs = registry
            .evaluate(&loaded.program, "Recover", inputs)
            .unwrap();
        assert_eq!(
            outputs["label"].as_ref().unwrap().get::<String>().unwrap(),
            expected
        );
        let value = ui::Several::builder().number(n).build(&graph).unwrap();
        let reason = value.value_fixed().unwrap().as_ref().unwrap_err();
        if n > 0.0 {
            assert_eq!(reason.value.get::<f32>().unwrap(), n)
        } else {
            assert_eq!(reason.value.get::<String>().unwrap(), "negative")
        }
    }
    assert!(
        ui::NoMatch::builder()
            .build(&graph)
            .unwrap_err()
            .to_string()
            .contains("no matching Organon case")
    );
    assert!(
        registry
            .evaluate(&loaded.program, "NoMatch", Default::default())
            .err()
            .unwrap()
            .contains("no matching Organon case")
    );
}
#[organon_theoria::implement]
impl Positive {
    fn test(n: &f32) -> bool {
        *n > 0.0
    }
}
#[test]
fn an_xml_element_owns_and_recovers_its_fallible_input() {
    let graph = theoria_reactive::Reactive::new();
    let text = graph.create_state(Err::<String, f32>(-1.0));
    let caption = ui::Caption::builder()
        .text(text.value())
        .build(&graph)
        .unwrap();
    let label: organon_theoria::Source<String> = caption.label();
    let raw: organon_theoria::Source<Result<String, f32>> = caption.raw();
    let read = graph.create_computed(move |native| {
        let cx = organon_theoria::ReadContext::new(native);
        (label.read(&cx).into_cloned(), raw.read(&cx).into_cloned())
    });
    assert_eq!(*read.snapshot().unwrap(), ("offline".into(), Err(-1.0)));
    text.set(Ok("online".into())).unwrap();
    assert_eq!(
        *read.snapshot().unwrap(),
        ("online".into(), Ok("online".into()))
    );

    let loaded =
        organon_cargo::load(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("organon.xml"))
            .unwrap();
    let registry = preview::registry();
    let reason = organon_theoria::flow::Failure::new::<organon_theoria::types::number>(-1.0);
    let inputs = [(
        "text".into(),
        Err(organon_theoria::flow::Error::Failure(reason)),
    )]
    .into();
    let outputs = registry
        .evaluate_with_inputs(&loaded.program, "Caption", inputs)
        .unwrap();
    assert_eq!(
        outputs["label"].as_ref().unwrap().get::<String>().unwrap(),
        "offline"
    );
    let Err(organon_theoria::flow::Error::Failure(raw)) = &outputs["raw"] else {
        panic!("expected the original input failure");
    };
    assert_eq!(raw.downcast::<organon_theoria::types::number>(), Some(-1.0));
    for (n, expected) in [(-1.0_f32, "offline"), (1.0, "ok")] {
        assert_eq!(
            ui::RiskCaption::builder()
                .n(n)
                .build(&graph)
                .unwrap()
                .caption_fixed()
                .unwrap(),
            expected
        );
        let inputs = [("n".into(), organon_app::evaluation::Value::new(n))].into();
        let outputs = registry
            .evaluate(&loaded.program, "RiskCaption", inputs)
            .unwrap();
        assert_eq!(
            outputs["caption"]
                .as_ref()
                .unwrap()
                .get::<String>()
                .unwrap(),
            expected
        );
    }
}
#[test]
fn unreachable_respects_handler_order_in_compiled_code_and_preview() {
    let graph = theoria_reactive::Reactive::new();
    let loaded =
        organon_cargo::load(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("organon.xml"))
            .unwrap();
    let registry = preview::registry();
    let preview = |n: f32| {
        let inputs = [("n".into(), organon_app::evaluation::Value::new(n))].into();
        registry.evaluate(&loaded.program, "CheckAssumption", inputs)
    };
    let build = |n: f32| ui::CheckAssumption::builder().n(n).build(&graph);
    assert_eq!(build(1.0).unwrap().label_fixed().unwrap(), "recovered");
    assert_eq!(
        preview(1.0).unwrap()["label"]
            .as_ref()
            .unwrap()
            .get::<String>()
            .unwrap(),
        "recovered"
    );
    for message in [
        build(-1.0).unwrap_err().to_string(),
        preview(-1.0).err().unwrap(),
    ] {
        assert!(
            message.contains("unreachable (handler 2)")
                && message.contains("{native}")
                && message.contains("number"),
            "{message}"
        );
    }
}
#[test]
#[should_panic(expected = "matched unreachable")]
fn a_later_reactive_failure_violates_the_same_assertion() {
    let graph = theoria_reactive::Reactive::new();
    let n = graph.create_state(1.0_f32);
    let label = ui::CheckAssumption::builder()
        .n(n.value())
        .build(&graph)
        .unwrap()
        .label();
    let read = graph.create_computed(move |cx| {
        label
            .read(&organon_theoria::ReadContext::new(cx))
            .into_cloned()
    });
    assert_eq!(*read.snapshot().unwrap(), "recovered");
    n.set(-1.0).unwrap();
    let _ = read.snapshot();
}
#[test]
fn guarded_catches_classify_the_same_failure_by_its_value() {
    let graph = theoria_reactive::Reactive::new();
    let loaded =
        organon_cargo::load(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("organon.xml"))
            .unwrap();
    let registry = preview::registry();
    for number in [1.0_f32, -2.0] {
        let built = ui::ClassifyError::builder()
            .number(number)
            .build(&graph)
            .unwrap();
        let compiled = built.label_fixed().unwrap().as_ref().unwrap_err();
        let inputs = [("number".into(), organon_app::evaluation::Value::new(number))].into();
        let outputs = registry
            .evaluate_typed(&loaded.program, "ClassifyError", inputs)
            .unwrap();
        let Err(organon_theoria::flow::Error::Failure(previewed)) = &outputs["label"] else {
            panic!("expected a typed failure");
        };
        assert_eq!(compiled.kind, previewed.kind);
        for error in [compiled, previewed] {
            if number > 0.0 {
                assert_eq!(error.value.get::<String>().unwrap(), "positive");
            } else {
                assert_eq!(error.value.get::<f32>().unwrap(), number);
            }
        }
    }
}
#[test]
fn switches_a_live_output_and_rethrows_past_sibling_handlers() {
    let graph = theoria_reactive::Reactive::new();
    let n = graph.create_state(1.0_f32);
    let object = ui::Control::builder()
        .number(n.value())
        .build(&graph)
        .unwrap();
    let label = object.label();
    let read = graph.create_computed(move |cx| {
        label
            .read(&organon_theoria::ReadContext::new(cx))
            .into_cloned()
    });
    assert_eq!(*read.snapshot().unwrap(), Ok("ready".to_owned()));
    n.set(-2.0).unwrap();
    assert_eq!(*read.snapshot().unwrap(), Err("negative".to_owned()));
    n.set(2.0).unwrap();
    assert_eq!(*read.snapshot().unwrap(), Ok("ready".to_owned()));
}
