use theoria_reactive::Reactive;

#[test]
fn snapshots_remain_valid_after_snapshot_state_changes() {
    let reactive = Reactive::new();
    let state = reactive.create_snapshot_state(String::from("before"));
    let before = state.snapshot().unwrap();

    state.set(String::from("after")).unwrap();

    assert_eq!(before.as_str(), "before");
    assert_eq!(before.cloned(), "before");
    assert_eq!(state.snapshot().unwrap().as_str(), "after");
}

#[test]
fn snapshots_outlive_their_snapshot_state_and_reactive() {
    let reactive = Reactive::new();
    let state = reactive.create_snapshot_state(String::from("retained"));
    let snapshot = state.snapshot().unwrap();
    drop(state);
    drop(reactive);

    assert_eq!(snapshot.as_str(), "retained");
}

#[test]
fn snapshot_state_can_be_updated_from_its_current_value() {
    let reactive = Reactive::new();
    let state = reactive.create_snapshot_state(String::from("before"));

    state.update(|current| format!("{current} after")).unwrap();

    assert_eq!(state.snapshot().unwrap().as_str(), "before after");
}

#[test]
fn snapshot_state_participates_in_tracked_evaluation() {
    let reactive = Reactive::new();
    let state = reactive.create_snapshot_state(2);
    let input = state.clone();
    let doubled = reactive.create_computed(move |cx| *cx.read(&input) * 2);

    assert_eq!(*doubled.snapshot().unwrap(), 4);
    reactive.mutate(|cx| cx.set(&state, 3)).unwrap();
    assert_eq!(*doubled.snapshot().unwrap(), 6);
    reactive
        .mutate(|cx| cx.update(&state, |value| value + 1))
        .unwrap();
    assert_eq!(*doubled.snapshot().unwrap(), 8);
}
