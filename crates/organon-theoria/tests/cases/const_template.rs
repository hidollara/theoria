use organon_theoria::{
    Calculation, ReadContext, Source, Type,
    attribute::{Scope, Value},
};

mod generated {
    organon_theoria::library!(crate::generated);
}

#[test]
fn const_and_dynamic_templates_agree_and_follow_multiple_inputs() {
    let reactive = theoria_reactive::Reactive::new();
    let label = reactive.create_state("Alice".to_owned());
    let count = reactive.create_state(2_f32);
    let scene = generated::Template::builder()
        .label(label.value())
        .count(count.value())
        .build(&reactive)
        .unwrap();
    let source = scene.text();
    let compiled =
        reactive.create_computed(move |cx| source.read(&ReadContext::new(cx)).into_cloned());

    let mut scope = Scope::new(organon_theoria::attribute_types());
    scope.declare_contract("later", "String").unwrap();
    scope.declare_contract("count", "number").unwrap();
    let label_source: Source<String> = label.value().into();
    let count_source: Source<f32> = count.value().into();
    scope.set("later", label_source.map(|s| Value::new(s.clone())));
    scope.set("count", count_source.map(|n| Value::new(*n)));
    let input = String::from("こんにちは {{{later}}} / {count} / {later} 🦀");
    let parsed = organon_theoria::types::String::interpret(&input, &scope).unwrap();
    assert_eq!(parsed.dependencies, ["count", "later"]);
    drop(input); // The constructed Source must not borrow the dynamic input or buffer.
    drop(scope);
    let source = Calculation::new(move |cx| parsed.source.read(cx).into_cloned())
        .bind(&reactive)
        .unwrap();
    let dynamic =
        reactive.create_computed(move |cx| source.read(&ReadContext::new(cx)).into_cloned());

    assert_eq!(
        &*compiled.snapshot().unwrap(),
        "こんにちは {Alice} / 2 / Alice 🦀"
    );
    assert_eq!(*compiled.snapshot().unwrap(), *dynamic.snapshot().unwrap());
    label.set("Bob".into()).unwrap();
    count.set(3.0).unwrap();
    assert_eq!(
        &*compiled.snapshot().unwrap(),
        "こんにちは {Bob} / 3 / Bob 🦀"
    );
    assert_eq!(*compiled.snapshot().unwrap(), *dynamic.snapshot().unwrap());
}

#[test]
fn zero_and_literal_only_storage_build_fixed_strings() {
    let reactive = theoria_reactive::Reactive::new();
    let literal = generated::Literal::builder().build(&reactive).unwrap();
    assert_eq!(
        literal.text_fixed().map(String::as_str),
        Some("日本語 {}\n\t🦀 ::theoria_reactive ::theoria_component")
    );
    let empty = generated::Empty::builder().build(&reactive).unwrap();
    assert_eq!(empty.text_fixed().map(String::as_str), Some(""));
}
