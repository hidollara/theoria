mod ui {
    organon_theoria::library!(crate::ui);
}
#[test]
fn type_interpretation_resolves_forward_names_without_treating_literals_as_references() {
    let reactive = theoria_reactive::Reactive::new();
    let forward = ui::ForwardAttribute::builder()
        .width(12.0)
        .build(&reactive)
        .unwrap();
    assert!(forward.path_fixed().unwrap().is_ok());
    assert!(
        ui::DecimalAttribute::builder()
            .build(&reactive)
            .unwrap()
            .path_fixed()
            .unwrap()
            .is_ok()
    );
    let literals = ui::LiteralNames::builder().build(&reactive).unwrap();
    assert_eq!(literals.a_fixed().unwrap(), "b");
    assert_eq!(literals.b_fixed().unwrap(), "a");
    assert!(
        ui::CyclicAttributes::builder()
            .build(&reactive)
            .err()
            .unwrap()
            .to_string()
            .contains("cyclic")
    );
}

#[test]
fn preview_uses_the_same_interpretation_and_dependency_order() {
    let loaded =
        organon_cargo::load(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("organon.xml"))
            .unwrap();
    let mut registry = preview::registry();
    for (local, id) in &loaded.loaded.origins {
        let identity = organon_cargo::identity(&id.source, &id.name).unwrap();
        let _ = registry.alias(local, &identity);
    }
    let program = loaded.program;

    let result = registry
        .evaluate(
            &program,
            "ForwardAttribute",
            [("width".into(), organon_app::evaluation::Value::new(12_f32))].into(),
        )
        .unwrap();
    assert!(
        result["path"]
            .as_ref()
            .unwrap()
            .get::<theoria_canvas::path::Path>()
            .is_ok()
    );
    assert!(
        registry
            .evaluate(&program, "DecimalAttribute", Default::default())
            .is_ok()
    );
    // Invalid numeric literals now fail static compilation. An edited document
    // must still report the same syntax error through dynamic interpretation.
    let mut edited = loaded.loaded.definitions.clone();
    for item in &mut edited.items {
        if let organon_core::syntax::Item::Definition(organon_core::syntax::Definition::Element(
            element,
        )) = item
        {
            if element.name == "DecimalAttribute" {
                for member in &mut element.members {
                    if let organon_core::syntax::ElementMember::Bind(binding) = member {
                        if let organon_core::syntax::ValueExpression::Construct(construction) =
                            &mut binding.value
                        {
                            if construction
                                .arguments
                                .get("width")
                                .is_some_and(|v| v == "100")
                            {
                                construction
                                    .arguments
                                    .insert("width".parse().unwrap(), "invalid".into());
                            }
                        }
                    }
                }
            }
        }
    }
    let edited = organon_core::semantic::resolve(&edited).unwrap();
    let error = registry
        .evaluate(&edited, "DecimalAttribute", Default::default())
        .err()
        .unwrap();
    assert!(
        error.contains("expected a signed decimal number"),
        "{error}"
    );
    let result = registry
        .evaluate(&program, "LiteralNames", Default::default())
        .unwrap();
    assert_eq!(result["a"].as_ref().unwrap().get::<String>().unwrap(), "b");
    assert_eq!(result["b"].as_ref().unwrap().get::<String>().unwrap(), "a");
    assert!(
        registry
            .evaluate(&program, "CyclicAttributes", Default::default())
            .err()
            .unwrap()
            .contains("cyclic")
    );
}

mod preview {
    organon_app::preview_library!("organon.xml");
}
