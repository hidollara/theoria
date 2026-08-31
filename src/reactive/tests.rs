use std::{
    cell::{Cell, RefCell},
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};

use super::{ComputeResult, ReactiveRuntime, Snapshot, State, Value};

mod effects;

fn resolve<T: 'static>(value: &Value<T>) -> Snapshot<T> {
    match &value.inner {
        super::value::ValueInner::Constant(value) => Snapshot::new(Rc::clone(value)),
        super::value::ValueInner::Reactive(handle) => {
            Snapshot::new(handle.runtime().resolve(handle.id))
        }
    }
}

fn set<T: 'static>(runtime: &ReactiveRuntime, state: &State<T>, value: T) {
    runtime.run_event_turn(|context| context.set(state, value));
}

fn update<T: 'static>(runtime: &ReactiveRuntime, state: &State<T>, update: impl FnOnce(&T) -> T) {
    runtime.run_event_turn(|context| context.update(state, update));
}

#[test]
fn state_exposes_a_read_only_value() {
    let runtime = ReactiveRuntime::new();
    let scope = runtime.scope();
    let state = scope.state(2);
    let value = state.value();

    assert_eq!(*resolve(&value), 2);
    set(&runtime, &state, 3);
    assert_eq!(*resolve(&value), 3);
    update(&runtime, &state, |current| current * 2);
    assert_eq!(*resolve(&value), 6);
}

#[test]
fn snapshots_remain_valid_after_state_changes() {
    let runtime = ReactiveRuntime::new();
    let scope = runtime.scope();
    let state = scope.state(String::from("before"));
    let value = state.value();
    let before = resolve(&value);

    set(&runtime, &state, String::from("after"));

    assert_eq!(before.as_str(), "before");
    assert_eq!(resolve(&value).as_str(), "after");
}

#[test]
fn value_clone_does_not_require_the_value_type_to_be_clone() {
    struct NotClone(u32);

    let fixed = Value::constant(NotClone(7));
    let cloned = fixed.clone();
    assert_eq!(resolve(&cloned).0, 7);
}

#[test]
fn computed_values_are_cached_until_a_dependency_changes() {
    let runtime = ReactiveRuntime::new();
    let scope = runtime.scope();
    let input = scope.state(2);
    let input_value = input.value();
    let evaluations = Rc::new(Cell::new(0));
    let evaluation_count = Rc::clone(&evaluations);
    let doubled = scope.computed(move |context| {
        evaluation_count.set(evaluation_count.get() + 1);
        *context.read(&input_value) * 2
    });

    assert_eq!(*resolve(&doubled), 4);
    assert_eq!(*resolve(&doubled), 4);
    assert_eq!(evaluations.get(), 1);

    set(&runtime, &input, 5);
    assert_eq!(*resolve(&doubled), 10);
    assert_eq!(*resolve(&doubled), 10);
    assert_eq!(evaluations.get(), 2);
}

#[test]
fn conditional_reads_replace_the_dependency_set() {
    let runtime = ReactiveRuntime::new();
    let scope = runtime.scope();
    let use_left = scope.state(true);
    let left = scope.state(10);
    let right = scope.state(20);
    let use_left_value = use_left.value();
    let left_value = left.value();
    let right_value = right.value();
    let evaluations = Rc::new(Cell::new(0));
    let evaluation_count = Rc::clone(&evaluations);
    let selected = scope.computed(move |context| {
        evaluation_count.set(evaluation_count.get() + 1);
        if *context.read(&use_left_value) {
            *context.read(&left_value)
        } else {
            *context.read(&right_value)
        }
    });

    assert_eq!(*resolve(&selected), 10);
    set(&runtime, &right, 21);
    assert_eq!(*resolve(&selected), 10);
    assert_eq!(evaluations.get(), 1);

    set(&runtime, &use_left, false);
    assert_eq!(*resolve(&selected), 21);
    assert_eq!(evaluations.get(), 2);

    set(&runtime, &left, 11);
    assert_eq!(*resolve(&selected), 21);
    assert_eq!(evaluations.get(), 2);

    set(&runtime, &right, 22);
    assert_eq!(*resolve(&selected), 22);
    assert_eq!(evaluations.get(), 3);
}

#[test]
fn incremental_computation_receives_the_previous_result() {
    let runtime = ReactiveRuntime::new();
    let scope = runtime.scope();
    let input = scope.state(1);
    let input_value = input.value();
    let previous_values = Rc::new(RefCell::new(Vec::new()));
    let observed_previous_values = Rc::clone(&previous_values);
    let doubled = scope.computed_incremental(move |context, previous| {
        observed_previous_values
            .borrow_mut()
            .push(previous.copied());
        let input = *context.read(&input_value);
        ComputeResult::Replace(input * 2)
    });

    assert_eq!(*resolve(&doubled), 2);
    set(&runtime, &input, 2);
    assert_eq!(*resolve(&doubled), 4);
    assert_eq!(&*previous_values.borrow(), &[None, Some(2)]);
}

#[test]
fn keeping_a_computed_result_stops_downstream_recomputation() {
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
    let downstream_evaluations = Rc::new(Cell::new(0));
    let evaluation_count = Rc::clone(&downstream_evaluations);
    let output = scope.computed(move |context| {
        evaluation_count.set(evaluation_count.get() + 1);
        *context.read(&stable) * 2
    });

    assert_eq!(*resolve(&output), 14);
    set(&runtime, &input, 2);
    assert_eq!(*resolve(&output), 14);
    assert_eq!(downstream_evaluations.get(), 1);
}

#[test]
fn one_runtime_allows_dependencies_across_scopes() {
    let runtime = ReactiveRuntime::new();
    let source_scope = runtime.scope();
    let output_scope = runtime.scope();
    let source = source_scope.state(3);
    let source_value = source.value();
    let output = output_scope.computed(move |context| *context.read(&source_value) + 1);

    assert_eq!(*resolve(&output), 4);
    set(&runtime, &source, 5);
    assert_eq!(*resolve(&output), 6);
}

#[test]
fn dependencies_cannot_cross_runtime_boundaries() {
    let first = ReactiveRuntime::new();
    let second = ReactiveRuntime::new();
    let first_scope = first.scope();
    let second_scope = second.scope();
    let source = first_scope.state(3).value();
    let output = second_scope.computed(move |context| *context.read(&source));

    assert!(catch_unwind(AssertUnwindSafe(|| resolve(&output))).is_err());
}

#[test]
fn dropping_a_scope_releases_all_of_its_nodes() {
    let runtime = ReactiveRuntime::new();
    {
        let scope = runtime.scope();
        let state = scope.state(1);
        let state_value = state.value();
        let _computed = scope.computed(move |context| *context.read(&state_value) + 1);
        assert_eq!(runtime.inner.node_count(), 2);
    }
    assert_eq!(runtime.inner.node_count(), 0);
}

#[test]
fn stale_handles_cannot_alias_reused_arena_slots() {
    let runtime = ReactiveRuntime::new();
    let stale = {
        let scope = runtime.scope();
        scope.state(1).value()
    };
    let replacement_scope = runtime.scope();
    let replacement = replacement_scope.state(2).value();

    assert_eq!(*resolve(&replacement), 2);
    assert!(catch_unwind(AssertUnwindSafe(|| resolve(&stale))).is_err());
}

#[test]
fn dependency_cycles_are_detected_without_poisoning_the_node() {
    let runtime = ReactiveRuntime::new();
    let scope = runtime.scope();
    let dependency = Rc::new(RefCell::new(None::<Value<i32>>));
    let builder_dependency = Rc::clone(&dependency);
    let computed = scope.computed(move |context| {
        let dependency = builder_dependency
            .borrow()
            .as_ref()
            .expect("test dependency")
            .clone();
        *context.read(&dependency)
    });
    dependency.replace(Some(computed.clone()));

    assert!(catch_unwind(AssertUnwindSafe(|| resolve(&computed))).is_err());

    dependency.replace(Some(Value::constant(9)));
    assert_eq!(*resolve(&computed), 9);
}

#[test]
fn a_panicking_builder_does_not_commit_partial_evaluation_state() {
    let runtime = ReactiveRuntime::new();
    let scope = runtime.scope();
    let input = scope.state(1);
    let should_panic = scope.state(false);
    let input_value = input.value();
    let should_panic_value = should_panic.value();
    let computed = scope.computed(move |context| {
        let input = *context.read(&input_value);
        assert!(!*context.read(&should_panic_value), "requested panic");
        input * 2
    });

    assert_eq!(*resolve(&computed), 2);
    runtime.run_event_turn(|context| {
        context.set(&input, 3);
        context.set(&should_panic, true);
    });
    assert!(catch_unwind(AssertUnwindSafe(|| resolve(&computed))).is_err());

    set(&runtime, &should_panic, false);
    assert_eq!(*resolve(&computed), 6);
}
