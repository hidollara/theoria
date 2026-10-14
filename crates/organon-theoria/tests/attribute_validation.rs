use organon_theoria::{
    Calculation, Source,
    attribute::Value,
    attribute::{Attribute, Scope},
};
fn interpret<M: organon_theoria::Type>(
    text: &str,
    scope: &Scope,
) -> Result<Attribute<Value>, String> {
    M::interpret(text, scope).map(|a| Attribute {
        dependencies: a.dependencies,
        source: a.source.map(|v| Value::new(v.clone())),
    })
}

#[test]
fn reference_formatting_contract_is_checked_before_evaluation() {
    let registry = organon_theoria::attribute_types();
    let mut scope = Scope::new(registry.clone());
    scope.declare("opaque");
    scope.set("opaque", Source::from(Value::new(vec![1_u8])));
    for text in ["{opaque}", "prefix {opaque}"] {
        let error = interpret::<organon_theoria::types::String>(text, &scope)
            .err()
            .unwrap();
        assert!(error.contains("does not satisfy"));
    }
    assert!(interpret::<organon_theoria::types::String>("{missing}", &scope).is_err());
    assert!(interpret::<organon_theoria::types::String>("{opaque", &scope).is_err());
}

#[test]
fn exact_typed_reference_does_not_require_display_but_interpolation_does() {
    #[derive(Clone, Debug, PartialEq)]
    struct Opaque(u32);
    struct OpaqueType;
    impl organon_theoria::Type for OpaqueType {
        type Value = Opaque;
    }
    let mut scope = Scope::default();
    scope.declare("item");
    scope.set("item", Source::from(Value::new(Opaque(7))));
    let reactive = theoria_reactive::Reactive::new();
    let direct = Attribute::reference::<OpaqueType>("item", &scope).unwrap();
    let bound = {
        let source = direct.source;
        organon_theoria::Calculation::new(move |cx| source.read(cx).into_cloned())
            .bind(&reactive, true)
            .unwrap()
    };
    assert_eq!(bound.as_fixed().unwrap(), &Opaque(7));
    let parsed =
        Attribute::parsed::<OpaqueType>("{item}", &scope, |_| Err("literal parsing must not run"))
            .unwrap();
    let bound = {
        let source = parsed.source;
        organon_theoria::Calculation::new(move |cx| source.read(cx).into_cloned())
            .bind(&reactive, true)
            .unwrap()
    };
    assert_eq!(bound.as_fixed().unwrap(), &Opaque(7));
}

#[test]
fn string_interpolation_tracks_updates_but_scalar_references_require_scalar() {
    let registry = organon_theoria::attribute_types();
    let mut scope = Scope::new(registry.clone());
    let reactive = theoria_reactive::Reactive::new();
    let number = reactive.create_state(12_f32);
    let value: Source<f32> = Source::from(number.value());
    scope.declare_typed::<f32>("number");
    scope.set(
        "number",
        Calculation::new(move |cx| Value::new(*value.read(cx))).into_inline(),
    );
    let attribute =
        interpret::<organon_theoria::types::String>("{{{number}}} / {number}", &scope).unwrap();
    assert_eq!(attribute.dependencies, ["number"]);
    let bound = {
        let source = attribute.source;
        organon_theoria::Calculation::new(move |cx| source.read(cx).into_cloned())
            .bind(&reactive, true)
            .unwrap()
    };
    assert!(bound.as_fixed().is_none());
    let value = bound.clone();
    let text = reactive.create_computed(move |cx| {
        value
            .read(&organon_theoria::ReadContext::new(cx))
            .get::<String>()
            .unwrap()
    });
    assert_eq!(&*text.snapshot().unwrap(), "{12} / 12");
    number.set(34.0).unwrap();
    assert_eq!(&*text.snapshot().unwrap(), "{34} / 34");

    scope.declare("text");
    scope.set("text", Source::from(Value::new("42".to_owned())));
    assert!(interpret::<organon_theoria::types::f32>("{text}", &scope).is_err());
    assert!(interpret::<organon_theoria::types::f32>("1{number}", &scope).is_err());
}

#[test]
fn custom_parser_checks_heterogeneous_forward_references_without_reading_values() {
    use organon_theoria::{attribute::ParseContext, types};
    let reactive = theoria_reactive::Reactive::new();
    let width = reactive.create_state(12_f32);
    let mut scope = Scope::new(organon_theoria::attribute_types());
    scope.declare_typed::<f32>("width");
    scope.declare_typed::<String>("label");
    let mut parser = ParseContext::new(&scope);
    assert!(parser.reference::<types::f32>("label").is_err());
    let w = parser.reference::<types::f32>("width").unwrap();
    let label = parser.reference::<types::String>("label").unwrap();
    let attribute = parser.finish(
        Calculation::new(move |cx| {
            let w = w.read(cx);
            let label = label.read(cx);
            format!("{}: {}", &*label, &*w)
        })
        .into_inline(),
    );
    assert_eq!(attribute.dependencies, ["label", "width"]);
    // Parsing succeeded before either binding was initialized.
    let input: Source<f32> = Source::from(width.value());
    scope.set(
        "width",
        Calculation::new(move |cx| Value::new(*input.read(cx))).into_inline(),
    );
    scope.set("label", Source::from(Value::new("W".to_owned())));
    let bound = {
        let source = attribute.source;
        organon_theoria::Calculation::new(move |cx| source.read(cx).into_cloned())
            .bind(&reactive, true)
            .unwrap()
    };
    let value = bound.clone();
    let text = reactive
        .create_computed(move |cx| value.read(&organon_theoria::ReadContext::new(cx)).clone());
    assert_eq!(&*text.snapshot().unwrap(), "W: 12");
    width.set(20.0).unwrap();
    assert_eq!(&*text.snapshot().unwrap(), "W: 20");
}

#[test]
fn literal_parser_finishes_with_a_fixed_value() {
    let registry = organon_theoria::attribute_types();
    let scope = Scope::new(registry.clone());
    let attribute = interpret::<organon_theoria::types::f32>("12", &scope).unwrap();
    assert_eq!(
        attribute.source.as_fixed().unwrap().get::<f32>().unwrap(),
        12.0
    );
    assert!(attribute.dependencies.is_empty());
}

#[test]
fn fallible_values_cannot_be_read_as_infallible_references() {
    use organon_theoria::{attribute::ParseContext, types};
    let mut scope = Scope::new(organon_theoria::attribute_types());
    scope.declare_typed::<Result<f32, &'static str>>("number");
    let mut parser = ParseContext::new(&scope);
    assert!(parser.reference::<types::f32>("number").is_err());
}

#[test]
fn direct_reference_requires_its_contract_before_any_value_is_available() {
    use organon_theoria::{attribute::ParseContext, types};
    let mut scope = Scope::new(organon_theoria::attribute_types());
    scope.declare_typed::<String>("label");
    let mut parser = ParseContext::new(&scope);
    assert!(parser.reference::<types::f32>("label").is_err());
    assert!(parser.reference::<types::String>("label").is_ok());
    assert!(parser.reference::<types::String>("missing").is_err());
}

#[test]
fn a_result_value_remains_a_result_and_can_recover_on_update() {
    let reactive = theoria_reactive::Reactive::new();
    let state = reactive.create_state(Err::<f32, &'static str>("not ready"));
    let mut scope = Scope::default();
    scope.declare_typed::<f32>("outcome");
    scope.declare_fallible("outcome");
    let source: Source<Result<f32, &'static str>> = Source::from(state.value());
    scope.set_fallible("outcome", source.map(|v| v.map(Value::new)));
    let mut parser = organon_theoria::attribute::ParseContext::new(&scope);
    assert!(
        parser
            .reference::<organon_theoria::types::f32>("outcome")
            .is_err()
    );
    let source: Source<Result<f32, &'static str>> = parser
        .reference_fallible::<organon_theoria::types::f32>("outcome")
        .unwrap();
    let result = reactive.create_computed(move |cx| {
        source
            .read(&organon_theoria::ReadContext::new(cx))
            .into_cloned()
    });
    assert_eq!(*result.snapshot().unwrap(), Err("not ready"));
    state.set(Ok(12.0)).unwrap();
    assert_eq!(*result.snapshot().unwrap(), Ok(12.0));
}
