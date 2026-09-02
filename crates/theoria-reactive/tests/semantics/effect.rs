use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use theoria_reactive::{Effect, Reactive, ReactiveError};

#[test]
fn effects_run_explicitly_and_unconditionally() {
    let reactive = Reactive::new();
    let state = reactive.create_state(1);
    let value = state.value();
    let observed = Rc::new(RefCell::new(Vec::new()));
    let effect_observed = Rc::clone(&observed);
    let effect = reactive.create_effect(move |cx| {
        effect_observed.borrow_mut().push(*cx.read(&value));
    });

    assert!(effect.is_stale());
    assert!(effect.run().is_ok());
    assert!(!effect.is_stale());
    assert!(effect.run().is_ok());
    assert_eq!(&*observed.borrow(), &[1, 1]);

    reactive
        .mutate(|cx| cx.set(&state, 2))
        .expect("mutation is available");
    assert!(effect.is_stale());
    assert_eq!(&*observed.borrow(), &[1, 1]);
    assert!(effect.run().is_ok());
    assert_eq!(&*observed.borrow(), &[1, 1, 2]);
}

#[test]
fn one_mutation_batches_multiple_state_writes_before_an_effect_run() {
    let reactive = Reactive::new();
    let left = reactive.create_state(1);
    let right = reactive.create_state(2);
    let left_value = left.value();
    let right_value = right.value();
    let observed = Rc::new(RefCell::new(Vec::new()));
    let effect_observed = Rc::clone(&observed);
    let effect = reactive.create_effect(move |cx| {
        effect_observed
            .borrow_mut()
            .push((*cx.read(&left_value), *cx.read(&right_value)));
    });
    assert!(effect.run().is_ok());

    reactive
        .mutate(|cx| {
            cx.set(&left, 10);
            cx.set(&right, 20);
            cx.modify(&left, |value| *value += 1);
        })
        .expect("mutation is available");

    assert_eq!(&*observed.borrow(), &[(1, 2)]);
    assert!(effect.is_stale());
    assert!(effect.run().is_ok());
    assert_eq!(&*observed.borrow(), &[(1, 2), (11, 20)]);
}

#[test]
fn effects_replace_conditional_dependencies() {
    let reactive = Reactive::new();
    let use_left = reactive.create_state(true);
    let left = reactive.create_state(1);
    let right = reactive.create_state(2);
    let use_left_value = use_left.value();
    let left_value = left.value();
    let right_value = right.value();
    let observed = Rc::new(RefCell::new(Vec::new()));
    let effect_observed = Rc::clone(&observed);
    let effect = reactive.create_effect(move |cx| {
        let selected = if *cx.read(&use_left_value) {
            *cx.read(&left_value)
        } else {
            *cx.read(&right_value)
        };
        effect_observed.borrow_mut().push(selected);
    });
    assert!(effect.run().is_ok());

    reactive
        .mutate(|cx| cx.set(&right, 3))
        .expect("mutation is available");
    assert!(!effect.is_stale());
    reactive
        .mutate(|cx| cx.set(&use_left, false))
        .expect("mutation is available");
    assert!(effect.is_stale());
    assert!(effect.run().is_ok());
    reactive
        .mutate(|cx| cx.set(&left, 4))
        .expect("mutation is available");
    assert!(!effect.is_stale());
    reactive
        .mutate(|cx| cx.set(&right, 5))
        .expect("mutation is available");
    assert!(effect.is_stale());
    assert!(effect.run().is_ok());
    assert_eq!(&*observed.borrow(), &[1, 3, 5]);
}

#[test]
fn pushed_effect_invalidation_does_not_evaluate_computed_values() {
    let reactive = Reactive::new();
    let input = reactive.create_state(1);
    let input_value = input.value();
    let evaluations = Rc::new(Cell::new(0));
    let computed_evaluations = Rc::clone(&evaluations);
    let doubled = reactive.create_computed(move |cx| {
        computed_evaluations.set(computed_evaluations.get() + 1);
        *cx.read(&input_value) * 2
    });
    let runs = Rc::new(Cell::new(0));
    let effect_runs = Rc::clone(&runs);
    let effect = reactive.create_effect(move |cx| {
        let _ = cx.read(&doubled);
        effect_runs.set(effect_runs.get() + 1);
    });
    assert!(effect.run().is_ok());
    assert_eq!(evaluations.get(), 1);

    reactive
        .mutate(|cx| cx.set(&input, 2))
        .expect("mutation is available");
    assert!(effect.is_stale());
    assert_eq!(evaluations.get(), 1);
    assert_eq!(runs.get(), 1);
    assert!(effect.run().is_ok());
    assert_eq!(evaluations.get(), 2);
    assert_eq!(runs.get(), 2);
}

#[test]
fn effect_identity_is_shared_by_clones_but_not_distinct_effects() {
    let reactive = Reactive::new();
    let first = reactive.create_effect(|_context| {});
    let clone = first.clone();
    let second = reactive.create_effect(|_context| {});

    assert_eq!(first, clone);
    assert_ne!(first, second);
}

#[test]
fn a_nested_top_level_effect_run_is_rejected_without_poisoning_the_effect() {
    let reactive = Rc::new(Reactive::new());
    let recursive_effect = Rc::new(RefCell::new(None::<Effect>));
    let callback_effect = Rc::clone(&recursive_effect);
    let callback_reactive = Rc::downgrade(&reactive);
    let recurse = Rc::new(Cell::new(true));
    let callback_recurse = Rc::clone(&recurse);
    let effect = reactive.create_effect(move |_context| {
        if callback_recurse.replace(false) {
            let _reactive = callback_reactive.upgrade().expect("test reactive");
            let effect = callback_effect
                .borrow()
                .as_ref()
                .expect("test effect")
                .clone();
            assert_eq!(effect.run(), Err(ReactiveError::AccessConflict));
        }
    });
    recursive_effect.replace(Some(effect.clone()));

    effect.run().unwrap();
    assert!(!effect.is_stale());
    assert!(effect.run().is_ok());
    assert!(!effect.is_stale());
}
