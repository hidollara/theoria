use std::{
    cell::Cell,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};

use theoria_reactive::{Reactive, ReactiveError};

#[test]
fn mutation_excludes_evaluation_and_other_batches_in_one_reactive() {
    let reactive = Reactive::new();
    let state = reactive.create_state(1);
    let state_reader = state.value();
    let observed = reactive.create_computed(move |cx| *cx.read(&state_reader));
    let effect = reactive.create_effect(|_| panic!("Effect must not run during mutation"));

    reactive
        .mutate(|cx| {
            assert_eq!(
                reactive.mutate(|_| panic!("nested mutation must not run")),
                Err(ReactiveError::AccessConflict)
            );
            assert_eq!(
                observed.snapshot().unwrap_err(),
                ReactiveError::AccessConflict
            );
            assert_eq!(effect.run(), Err(ReactiveError::AccessConflict));
            assert_eq!(state.read().unwrap_err(), ReactiveError::AccessConflict);
            cx.set(&state, 2);
        })
        .unwrap();

    assert_eq!(*observed.snapshot().unwrap(), 2);
}

#[test]
fn computed_and_effect_evaluation_exclude_other_top_level_operations() {
    let reactive = Reactive::new();
    let state = reactive.create_state(1);
    let input = state.clone();
    let writer = reactive.clone();
    let computed = reactive.create_computed(move |cx| {
        assert_eq!(
            writer.mutate(|_| panic!("mutation during Computed")),
            Err(ReactiveError::AccessConflict)
        );
        assert_eq!(input.read().unwrap_err(), ReactiveError::AccessConflict);
        *cx.read(&input) + 1
    });

    let observed = Rc::new(Cell::new(0));
    let output = Rc::clone(&observed);
    let writer = reactive.clone();
    let effect_state = state.clone();
    let effect = reactive.create_effect(move |cx| {
        assert_eq!(
            writer.mutate(|_| panic!("mutation during Effect")),
            Err(ReactiveError::AccessConflict)
        );
        assert_eq!(
            effect_state.read().unwrap_err(),
            ReactiveError::AccessConflict
        );
        output.set(*cx.read(&computed));
    });

    effect.run().unwrap();
    assert_eq!(observed.get(), 2);
    state.set(3).unwrap();
    effect.run().unwrap();
    assert_eq!(observed.get(), 4);
}

#[test]
fn direct_state_read_excludes_other_top_level_operations_until_dropped() {
    let reactive = Reactive::new();
    let state = reactive.create_state(String::from("before"));
    let input = state.value();
    let length = reactive.create_computed(move |cx| cx.read(&input).len());
    let effect = reactive.create_effect(|_| {});

    let current = state.read().unwrap();
    assert_eq!(
        state.set(String::from("blocked")),
        Err(ReactiveError::AccessConflict)
    );
    assert_eq!(
        length.snapshot().unwrap_err(),
        ReactiveError::AccessConflict
    );
    assert_eq!(effect.run(), Err(ReactiveError::AccessConflict));

    drop(current);
    state.set(String::from("after")).unwrap();
    assert_eq!(*length.snapshot().unwrap(), 5);
    assert!(effect.run().is_ok());
}

#[test]
fn a_panicking_mutation_releases_access_without_rolling_back_completed_writes() {
    let reactive = Reactive::new();
    let state = reactive.create_state(1);
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            reactive.mutate(|cx| {
                cx.set(&state, 2);
                panic!("requested mutation panic");
            })
        }))
        .is_err()
    );
    let state_reader = state.value();
    let observed = reactive.create_computed(move |cx| *cx.read(&state_reader));
    assert_eq!(*observed.snapshot().unwrap(), 2);
    state.set(3).unwrap();
    assert_eq!(*observed.snapshot().unwrap(), 3);
}

#[test]
fn computed_evaluation_cannot_start_another_top_level_evaluation() {
    let reactive = Reactive::new();
    let state = reactive.create_state(1);
    let state_reader = state.value();
    let direct = reactive.create_computed(move |cx| *cx.read(&state_reader));
    let computed = reactive.create_computed(move |_| {
        assert_eq!(
            direct.snapshot().unwrap_err(),
            ReactiveError::AccessConflict
        );
        2
    });
    assert_eq!(*computed.snapshot().unwrap(), 2);
}
