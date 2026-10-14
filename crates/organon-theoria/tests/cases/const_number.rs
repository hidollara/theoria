use organon_theoria::{
    Calculation, ReadContext, Source, Type,
    attribute::{Scope, Value},
};

mod generated {
    organon_theoria::library!(crate::generated);
}

#[test]
fn static_and_dynamic_literals_round_to_the_same_fixed_value() {
    let reactive = theoria_reactive::Reactive::new();
    let literal = generated::Literal::builder().build(&reactive).unwrap();
    assert_eq!(literal.number_fixed().unwrap().to_bits(), 0x3f80_0001);
    let scope = Scope::new(organon_theoria::attribute_types());
    let parsed =
        organon_theoria::types::number::interpret("1.00000005960464477539062500001", &scope)
            .unwrap();
    assert!(parsed.dependencies.is_empty());
    let source = Calculation::new(move |cx| parsed.source.read(cx).into_cloned())
        .bind(&reactive)
        .unwrap();
    assert_eq!(source.as_fixed().unwrap().to_bits(), 0x3f80_0001);
    for text in ["NaN", "1e2", "340282356779733661637539395458142568448"] {
        assert!(organon_theoria::types::number::interpret(text, &scope).is_err());
    }
}

#[test]
fn references_are_checked_without_reading_and_still_follow_updates() {
    let reactive = theoria_reactive::Reactive::new();
    let input = reactive.create_state(10_f32);
    let element = generated::Forward::builder()
        .input(input.value())
        .build(&reactive)
        .unwrap();
    let source = element.number();
    let compiled =
        reactive.create_computed(move |cx| source.read(&ReadContext::new(cx)).into_cloned());

    let mut scope = Scope::new(organon_theoria::attribute_types());
    scope.declare_contract("later", "number").unwrap();
    let source: Source<f32> = input.value().into();
    scope.set("later", source.map(|value| Value::new(*value)));
    let parsed = organon_theoria::types::number::interpret("{later}", &scope).unwrap();
    assert_eq!(parsed.dependencies, ["later"]);
    let source = Calculation::new(move |cx| parsed.source.read(cx).into_cloned())
        .bind(&reactive)
        .unwrap();
    let dynamic =
        reactive.create_computed(move |cx| source.read(&ReadContext::new(cx)).into_cloned());
    assert_eq!(*compiled.snapshot().unwrap(), 10.0);
    assert_eq!(*dynamic.snapshot().unwrap(), 10.0);
    input.set(20.0).unwrap();
    assert_eq!(*compiled.snapshot().unwrap(), 20.0);
    assert_eq!(*dynamic.snapshot().unwrap(), 20.0);
}
