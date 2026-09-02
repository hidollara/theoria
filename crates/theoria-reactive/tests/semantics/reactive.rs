use std::panic::{AssertUnwindSafe, catch_unwind};

use theoria_reactive::Reactive;

#[test]
fn reactive_clones_create_compatible_values() {
    let reactive = Reactive::new();
    let clone = reactive.clone();
    let state = reactive.create_state(1);
    let input = state.value();
    let computed = clone.create_computed(move |cx| *cx.read(&input) + 1);
    assert_eq!(*computed.snapshot().unwrap(), 2);
}

#[test]
fn independent_reactives_reject_cross_reactive_reads_and_writes() {
    let first = Reactive::new();
    let second = Reactive::new();
    let state = first.create_state(1);
    let input = state.value();
    let observed_input = state.value();
    let observed = first.create_computed(move |cx| *cx.read(&observed_input));
    let computed = second.create_computed(move |cx| *cx.read(&input));

    assert!(catch_unwind(AssertUnwindSafe(|| computed.snapshot())).is_err());
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            second.mutate(|cx| cx.set(&state, 2)).unwrap();
        }))
        .is_err()
    );
    assert_eq!(*observed.snapshot().unwrap(), 1);
}
