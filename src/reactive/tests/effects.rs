use super::*;

#[test]
fn effects_run_initially_and_after_published_dependency_changes() {
    let runtime = ReactiveRuntime::new();
    let scope = runtime.scope();
    let state = scope.state(1);
    let value = state.value();
    let observed = Rc::new(RefCell::new(Vec::new()));
    let effect_observed = Rc::clone(&observed);
    scope.effect(move |context| {
        effect_observed.borrow_mut().push(*context.read(&value));
    });

    assert!(observed.borrow().is_empty());
    runtime.flush_effects();
    assert_eq!(&*observed.borrow(), &[1]);
    runtime.flush_effects();
    assert_eq!(&*observed.borrow(), &[1]);

    set(&runtime, &state, 2);
    assert_eq!(&*observed.borrow(), &[1, 2]);
}

#[test]
fn one_event_turn_batches_multiple_state_writes_before_effects() {
    let runtime = ReactiveRuntime::new();
    let scope = runtime.scope();
    let left = scope.state(1);
    let right = scope.state(2);
    let left_value = left.value();
    let right_value = right.value();
    let observed = Rc::new(RefCell::new(Vec::new()));
    let effect_observed = Rc::clone(&observed);
    scope.effect(move |context| {
        effect_observed
            .borrow_mut()
            .push((*context.read(&left_value), *context.read(&right_value)));
    });
    runtime.flush_effects();

    runtime.run_event_turn(|context| {
        context.set(&left, 10);
        context.set(&right, 20);
        context.update(&left, |value| value + 1);
    });

    assert_eq!(&*observed.borrow(), &[(1, 2), (11, 20)]);
}

#[test]
fn event_reads_are_untracked_current_snapshots() {
    let runtime = ReactiveRuntime::new();
    let scope = runtime.scope();
    let state = scope.state(1);
    let value = state.value();

    runtime.run_event_turn(|context| {
        assert_eq!(*context.read_current(&value), 1);
        context.set(&state, 2);
        assert_eq!(*context.read_current(&value), 2);
    });
}

#[test]
fn effects_replace_conditional_dependencies() {
    let runtime = ReactiveRuntime::new();
    let scope = runtime.scope();
    let use_left = scope.state(true);
    let left = scope.state(1);
    let right = scope.state(2);
    let use_left_value = use_left.value();
    let left_value = left.value();
    let right_value = right.value();
    let observed = Rc::new(RefCell::new(Vec::new()));
    let effect_observed = Rc::clone(&observed);
    scope.effect(move |context| {
        let selected = if *context.read(&use_left_value) {
            *context.read(&left_value)
        } else {
            *context.read(&right_value)
        };
        effect_observed.borrow_mut().push(selected);
    });
    runtime.flush_effects();

    set(&runtime, &right, 3);
    assert_eq!(&*observed.borrow(), &[1]);
    set(&runtime, &use_left, false);
    assert_eq!(&*observed.borrow(), &[1, 3]);
    set(&runtime, &left, 4);
    assert_eq!(&*observed.borrow(), &[1, 3]);
    set(&runtime, &right, 5);
    assert_eq!(&*observed.borrow(), &[1, 3, 5]);
}

#[test]
fn kept_computed_results_do_not_rerun_effects() {
    let runtime = ReactiveRuntime::new();
    let scope = runtime.scope();
    let input = scope.state(1);
    let input_value = input.value();
    let stable = scope.computed_incremental(move |context, previous| {
        let _ = context.read(&input_value);
        match previous {
            Some(_) => ComputeResult::Keep,
            None => ComputeResult::Replace(7),
        }
    });
    let runs = Rc::new(Cell::new(0));
    let effect_runs = Rc::clone(&runs);
    scope.effect(move |context| {
        let _ = context.read(&stable);
        effect_runs.set(effect_runs.get() + 1);
    });
    runtime.flush_effects();

    set(&runtime, &input, 2);
    assert_eq!(runs.get(), 1);
}

#[test]
fn effects_run_in_registration_order_even_after_slot_reuse() {
    let runtime = ReactiveRuntime::new();
    let state_scope = runtime.scope();
    let state = state_scope.state(0);
    let value = state.value();
    let order = Rc::new(RefCell::new(Vec::new()));

    let first_scope = runtime.scope();
    let first_value = value.clone();
    let first_order = Rc::clone(&order);
    first_scope.effect(move |context| {
        let _ = context.read(&first_value);
        first_order.borrow_mut().push("first");
    });

    let second_scope = runtime.scope();
    let second_value = value.clone();
    let second_order = Rc::clone(&order);
    second_scope.effect(move |context| {
        let _ = context.read(&second_value);
        second_order.borrow_mut().push("second");
    });
    runtime.flush_effects();
    order.borrow_mut().clear();

    drop(first_scope);
    let third_scope = runtime.scope();
    let third_order = Rc::clone(&order);
    third_scope.effect(move |context| {
        let _ = context.read(&value);
        third_order.borrow_mut().push("third");
    });

    set(&runtime, &state, 1);
    assert_eq!(&*order.borrow(), &["second", "third"]);
}

#[test]
fn dropping_a_scope_unregisters_its_effects() {
    let runtime = ReactiveRuntime::new();
    let runs = Rc::new(Cell::new(0));
    {
        let scope = runtime.scope();
        let effect_runs = Rc::clone(&runs);
        scope.effect(move |_context| effect_runs.set(effect_runs.get() + 1));
        assert_eq!(runtime.inner.effect_count(), 1);
        runtime.flush_effects();
    }

    assert_eq!(runtime.inner.effect_count(), 0);
    runtime.flush_effects();
    assert_eq!(runs.get(), 1);
}

#[test]
fn runtime_rejects_nested_event_turns_without_becoming_stuck() {
    let runtime = ReactiveRuntime::new();
    let nested_runtime = runtime.clone();

    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            runtime.run_event_turn(|_context| {
                nested_runtime.run_event_turn(|_context| {});
            });
        }))
        .is_err()
    );
    runtime.run_event_turn(|_context| {});
}

#[test]
fn effects_cannot_start_a_reentrant_event_turn() {
    let runtime = ReactiveRuntime::new();
    let scope = runtime.scope();
    let effect_runtime = runtime.clone();
    scope.effect(move |_context| {
        effect_runtime.run_event_turn(|_context| {});
    });

    assert!(catch_unwind(AssertUnwindSafe(|| runtime.flush_effects())).is_err());
    drop(scope);

    let separate_scope = runtime.scope();
    let ran = Rc::new(Cell::new(false));
    let effect_ran = Rc::clone(&ran);
    separate_scope.effect(move |_context| effect_ran.set(true));
    runtime.flush_effects();
    assert!(ran.get());
}

#[test]
fn contexts_reject_handles_from_other_runtimes() {
    let first = ReactiveRuntime::new();
    let second = ReactiveRuntime::new();
    let first_scope = first.scope();
    let second_scope = second.scope();
    let first_state = first_scope.state(1);
    let second_value = second_scope.state(2).value();

    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            second.run_event_turn(|context| context.set(&first_state, 3));
        }))
        .is_err()
    );

    first_scope.effect(move |context| {
        let _ = context.read(&second_value);
    });
    assert!(catch_unwind(AssertUnwindSafe(|| first.flush_effects())).is_err());
}
