use theoria_reactive::Reactive;

#[test]
fn read_context_accepts_all_public_value_handles() {
    let reactive = Reactive::new();
    let state = reactive.create_state(2);
    let snapshot_state = reactive.create_snapshot_state(3);
    let state_writer = state.clone();
    let snapshot_state_writer = snapshot_state.clone();
    let state_value = state.value();
    let computed = reactive.create_computed(move |cx| *cx.read(&state_value) * 5);
    let computed_handle = computed.clone();
    let computed_value = computed.value();

    let result = reactive.create_computed(move |cx| {
        *cx.read(&state)
            + *cx.read(&snapshot_state)
            + *cx.read(&computed_handle)
            + *cx.read(&computed_value)
    });

    assert_eq!(*result.snapshot().unwrap(), 25);

    state_writer.set(4).unwrap();
    assert_eq!(*result.snapshot().unwrap(), 47);
    snapshot_state_writer.set(6).unwrap();
    assert_eq!(*result.snapshot().unwrap(), 50);
}
