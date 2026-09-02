use theoria_reactive::Reactive;

#[test]
fn state_can_be_borrowed_without_tracking_a_dependency() {
    let reactive = Reactive::new();
    let state = reactive.create_state(String::from("before"));

    let current = state.read().unwrap();
    assert_eq!(current.as_str(), "before");
    assert_eq!(current.cloned(), "before");
}

#[test]
fn state_supports_direct_set_modify_update_and_replace_operations() {
    let reactive = Reactive::new();
    let state = reactive.create_state(2);
    let value = state.value();
    let observed = reactive.create_computed(move |cx| *cx.read(&value));

    state.set(3).expect("mutation is available");
    assert_eq!(*observed.snapshot().expect("evaluation is available"), 3);

    state
        .modify(|current| *current *= 2)
        .expect("mutation is available");
    assert_eq!(*observed.snapshot().expect("evaluation is available"), 6);

    state
        .update(|current| current + 2)
        .expect("mutation is available");
    assert_eq!(*observed.snapshot().expect("evaluation is available"), 8);

    assert_eq!(state.replace(9).expect("mutation is available"), 8);
    assert_eq!(*observed.snapshot().expect("evaluation is available"), 9);
}

#[test]
fn state_mutation_reuses_and_returns_non_clone_values() {
    struct NotClone(Vec<u32>);

    let reactive = Reactive::new();
    let state = reactive.create_state(NotClone(vec![1]));
    let value = state.value();
    let length = reactive.create_computed(move |cx| cx.read(&value).0.len());

    state.modify(|value| value.0.push(2)).unwrap();
    assert_eq!(*length.snapshot().unwrap(), 2);

    let previous = state.replace(NotClone(vec![3])).unwrap();
    assert_eq!(previous.0, vec![1, 2]);
    assert_eq!(*length.snapshot().unwrap(), 1);
}
