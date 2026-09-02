use std::{
    cell::{Cell, RefCell},
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};

use theoria_reactive::{Computed, Reactive};

#[test]
fn computed_values_are_cached_until_a_dependency_changes() {
    let reactive = Reactive::new();
    let input = reactive.create_state(2);
    let input_value = input.value();
    let evaluations = Rc::new(Cell::new(0));
    let evaluation_count = Rc::clone(&evaluations);
    let doubled = reactive.create_computed(move |cx| {
        evaluation_count.set(evaluation_count.get() + 1);
        *cx.read(&input_value) * 2
    });

    assert_eq!(*doubled.snapshot().expect("evaluation is available"), 4);
    assert_eq!(*doubled.snapshot().expect("evaluation is available"), 4);
    assert_eq!(evaluations.get(), 1);

    reactive
        .mutate(|cx| cx.set(&input, 5))
        .expect("mutation is available");
    assert_eq!(evaluations.get(), 1);
    assert_eq!(*doubled.snapshot().expect("evaluation is available"), 10);
    assert_eq!(*doubled.snapshot().expect("evaluation is available"), 10);
    assert_eq!(evaluations.get(), 2);
}

#[test]
fn computed_is_invalidated_when_a_state_is_replaced_with_an_equal_value() {
    let reactive = Reactive::new();
    let state = reactive.create_state(1);
    let read_state = state.clone();
    let evaluations = Rc::new(Cell::new(0));
    let evaluation_count = Rc::clone(&evaluations);
    let computed = reactive.create_computed(move |cx| {
        evaluation_count.set(evaluation_count.get() + 1);
        *cx.read(&read_state)
    });

    assert_eq!(*computed.snapshot().expect("evaluation is available"), 1);
    reactive
        .mutate(|cx| cx.set(&state, 1))
        .expect("mutation is available");
    assert_eq!(evaluations.get(), 1);
    assert_eq!(*computed.snapshot().expect("evaluation is available"), 1);
    assert_eq!(evaluations.get(), 2);
}

#[test]
fn pull_evaluation_orders_a_dirty_diamond_and_evaluates_each_value_once() {
    let reactive = Reactive::new();
    let input = reactive.create_state(2);
    let input_value = input.value();

    let left_evaluations = Rc::new(Cell::new(0));
    let left_count = Rc::clone(&left_evaluations);
    let left = reactive.create_computed(move |cx| {
        left_count.set(left_count.get() + 1);
        *cx.read(&input_value) + 1
    });

    let input_value = input.value();
    let right_evaluations = Rc::new(Cell::new(0));
    let right_count = Rc::clone(&right_evaluations);
    let right = reactive.create_computed(move |cx| {
        right_count.set(right_count.get() + 1);
        *cx.read(&input_value) * 2
    });

    let output_evaluations = Rc::new(Cell::new(0));
    let output_count = Rc::clone(&output_evaluations);
    let output = reactive.create_computed(move |cx| {
        output_count.set(output_count.get() + 1);
        *cx.read(&left) + *cx.read(&right)
    });

    assert_eq!(*output.snapshot().expect("evaluation is available"), 7);
    reactive
        .mutate(|cx| cx.set(&input, 4))
        .expect("mutation is available");
    assert_eq!(left_evaluations.get(), 1);
    assert_eq!(right_evaluations.get(), 1);
    assert_eq!(output_evaluations.get(), 1);

    assert_eq!(*output.snapshot().expect("evaluation is available"), 13);
    assert_eq!(left_evaluations.get(), 2);
    assert_eq!(right_evaluations.get(), 2);
    assert_eq!(output_evaluations.get(), 2);
}

#[test]
fn conditional_reads_replace_the_dependency_set() {
    let reactive = Reactive::new();
    let use_left = reactive.create_state(true);
    let left = reactive.create_state(10);
    let right = reactive.create_state(20);
    let use_left_value = use_left.value();
    let left_value = left.value();
    let right_value = right.value();
    let evaluations = Rc::new(Cell::new(0));
    let evaluation_count = Rc::clone(&evaluations);
    let selected = reactive.create_computed(move |cx| {
        evaluation_count.set(evaluation_count.get() + 1);
        if *cx.read(&use_left_value) {
            *cx.read(&left_value)
        } else {
            *cx.read(&right_value)
        }
    });

    assert_eq!(*selected.snapshot().expect("evaluation is available"), 10);
    reactive
        .mutate(|cx| cx.set(&right, 21))
        .expect("mutation is available");
    assert_eq!(*selected.snapshot().expect("evaluation is available"), 10);
    assert_eq!(evaluations.get(), 1);

    reactive
        .mutate(|cx| cx.set(&use_left, false))
        .expect("mutation is available");
    assert_eq!(*selected.snapshot().expect("evaluation is available"), 21);
    assert_eq!(evaluations.get(), 2);

    reactive
        .mutate(|cx| cx.set(&left, 11))
        .expect("mutation is available");
    assert_eq!(*selected.snapshot().expect("evaluation is available"), 21);
    assert_eq!(evaluations.get(), 2);

    reactive
        .mutate(|cx| cx.set(&right, 22))
        .expect("mutation is available");
    assert_eq!(*selected.snapshot().expect("evaluation is available"), 22);
    assert_eq!(evaluations.get(), 3);
}

#[test]
fn dependency_cycles_are_detected_without_poisoning_the_value() {
    let reactive = Reactive::new();
    let fallback_state = reactive.create_state(9);
    let fallback_reader = fallback_state.value();
    let fallback = reactive.create_computed(move |cx| *cx.read(&fallback_reader));
    let dependency = Rc::new(RefCell::new(None::<Computed<i32>>));
    let builder_dependency = Rc::clone(&dependency);
    let computed = reactive.create_computed(move |cx| {
        let dependency = builder_dependency
            .borrow()
            .as_ref()
            .expect("test dependency")
            .clone();
        *cx.read(&dependency)
    });
    dependency.replace(Some(computed.clone()));

    assert!(
        catch_unwind(AssertUnwindSafe(|| computed
            .snapshot()
            .expect("evaluation is available")))
        .is_err()
    );

    dependency.replace(Some(fallback));
    assert_eq!(*computed.snapshot().expect("evaluation is available"), 9);
}

#[test]
fn computed_clone_does_not_require_the_result_type_to_be_clone() {
    struct NotClone(u32);

    let reactive = Reactive::new();
    let computed = reactive.create_computed(|_| NotClone(7));
    let cloned = computed.clone();

    assert_eq!(cloned.snapshot().unwrap().0, 7);
}
