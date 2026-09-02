use theoria_reactive::Reactive;

#[test]
fn write_context_batches_several_state_mutations() {
    let reactive = Reactive::new();
    let left = reactive.create_state(1);
    let right = reactive.create_state(2);
    let left_value = left.value();
    let right_value = right.value();
    let observed =
        reactive.create_computed(move |cx| (*cx.read(&left_value), *cx.read(&right_value)));

    reactive
        .mutate(|cx| {
            cx.set(&left, 3);
            cx.update(&left, |value| value + 1);
            cx.modify(&right, |value| *value *= 2);
            assert_eq!(cx.replace(&right, 5), 4);
        })
        .unwrap();

    assert_eq!(*observed.snapshot().unwrap(), (4, 5));
}
