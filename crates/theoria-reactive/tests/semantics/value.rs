use theoria_reactive::{Reactive, Value};

#[test]
fn value_preserves_read_access_without_exposing_state_mutation() {
    let reactive = Reactive::new();
    let state = reactive.create_state(2);
    let value: Value<_> = state.value();
    let observed = reactive.create_computed(move |cx| *cx.read(&value));

    assert_eq!(*observed.snapshot().unwrap(), 2);
    state.set(3).unwrap();
    assert_eq!(*observed.snapshot().unwrap(), 3);
}
