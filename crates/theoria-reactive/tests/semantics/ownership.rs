use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use theoria_reactive::Reactive;

#[test]
fn a_value_keeps_its_state_storage_alive_without_the_reactive_or_state() {
    let reactive = Reactive::new();
    let state = reactive.create_state(1);
    let value = state.value();
    let observed = Rc::new(Cell::new(0));
    let effect_observed = Rc::clone(&observed);
    let effect = reactive.create_effect(move |cx| {
        effect_observed.set(*cx.read(&value));
    });
    drop(reactive);
    drop(state);

    effect.run().unwrap();
    assert_eq!(observed.get(), 1);
}

#[test]
fn an_effect_handle_remains_runnable_without_the_reactive() {
    let reactive = Reactive::new();
    let runs = Rc::new(Cell::new(0));
    let effect_runs = Rc::clone(&runs);
    let effect = reactive.create_effect(move |_context| effect_runs.set(effect_runs.get() + 1));
    assert!(effect.run().is_ok());
    drop(reactive);

    assert!(!effect.is_stale());
    assert!(effect.run().is_ok());
    assert_eq!(runs.get(), 2);
}

#[test]
fn computed_dependencies_keep_their_input_values_alive() {
    struct CountDrop(Rc<Cell<usize>>);
    impl Drop for CountDrop {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    let reactive = Reactive::new();
    let drops = Rc::new(Cell::new(0));
    let state = reactive.create_state(CountDrop(Rc::clone(&drops)));
    let input = Rc::new(RefCell::new(Some(state.value())));
    let builder_input = Rc::clone(&input);
    let computed = reactive.create_computed(move |cx| {
        let input = builder_input.borrow();
        let _ = cx.read(input.as_ref().expect("test input is installed"));
        1
    });

    assert_eq!(*computed.snapshot().unwrap(), 1);
    input.borrow_mut().take();
    drop(state);
    drop(reactive);
    assert_eq!(drops.get(), 0);
    drop(computed);
    assert_eq!(drops.get(), 1);
}

#[test]
fn dropping_the_last_computed_handle_releases_its_cached_value_and_dependencies() {
    struct CountDrop(Rc<Cell<usize>>);
    impl Drop for CountDrop {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    let reactive = Reactive::new();
    let source_drops = Rc::new(Cell::new(0));
    let cached_drops = Rc::new(Cell::new(0));
    let source = reactive.create_state(CountDrop(Rc::clone(&source_drops)));
    let input = source.value();
    let output_drops = Rc::clone(&cached_drops);
    let computed = reactive.create_computed(move |cx| {
        let _ = cx.read(&input);
        CountDrop(Rc::clone(&output_drops))
    });
    computed.snapshot().unwrap();

    drop(source);
    drop(reactive);
    assert_eq!(source_drops.get(), 0);
    assert_eq!(cached_drops.get(), 0);
    drop(computed);
    assert_eq!(source_drops.get(), 1);
    assert_eq!(cached_drops.get(), 1);
}

#[test]
fn invalidating_a_computed_releases_its_cached_value() {
    struct CountDrop(Rc<Cell<usize>>);
    impl Drop for CountDrop {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    let reactive = Reactive::new();
    let source = reactive.create_state(1);
    let input = source.value();
    let cached_drops = Rc::new(Cell::new(0));
    let output_drops = Rc::clone(&cached_drops);
    let computed = reactive.create_computed(move |cx| {
        let _ = cx.read(&input);
        CountDrop(Rc::clone(&output_drops))
    });

    let snapshot = computed.snapshot().unwrap();
    drop(snapshot);
    assert_eq!(cached_drops.get(), 0);

    source.set(2).unwrap();
    assert_eq!(cached_drops.get(), 1);
}

#[test]
fn the_last_computed_handle_keeps_an_acyclic_dependency_chain_alive() {
    let reactive = Reactive::new();
    let source = reactive.create_state(1);
    let first_input = source.value();
    let first = reactive.create_computed(move |cx| *cx.read(&first_input) + 1);
    let second_input = first.clone();
    let second = reactive.create_computed(move |cx| *cx.read(&second_input) + 1);
    assert_eq!(*second.snapshot().unwrap(), 3);
    drop(source);
    drop(first);
    drop(reactive);
    assert_eq!(*second.snapshot().unwrap(), 3);
}
