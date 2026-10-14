use organon_app::evaluation::Source as Value;
use std::{cell::Cell, rc::Rc};
use theoria_reactive::Reactive;

#[test]
fn into_cloned_moves_inline_results_and_clones_borrowed_values() {
    struct Counted {
        clones: Rc<Cell<usize>>,
    }
    impl Clone for Counted {
        fn clone(&self) -> Self {
            self.clones.set(self.clones.get() + 1);
            Self {
                clones: self.clones.clone(),
            }
        }
    }
    for kind in 0..4 {
        let reactive = Reactive::new();
        let clones = Rc::new(Cell::new(0));
        let count = clones.clone();
        let source = match kind {
            0 => Value::fixed(Counted { clones: count }),
            1 => Value::reactive(reactive.create_state(Counted { clones: count }).value()),
            2 => Value::reactive(
                reactive
                    .create_computed(move |_| Counted {
                        clones: count.clone(),
                    })
                    .value(),
            ),
            _ => Value::inline(move |_| Counted {
                clones: count.clone(),
            }),
        };
        let reader = reactive.create_computed(move |cx| source.read(cx).into_cloned());
        let retained = reader.snapshot().unwrap();
        drop(reader);
        assert!(Rc::ptr_eq(&retained.clones, &clones));
        assert_eq!(clones.get(), if kind == 3 { 0 } else { 1 });
    }
}

#[test]
fn all_value_kinds_can_be_read_without_clone() {
    struct NonClone(i32);
    let reactive = Reactive::new();
    let fixed = Value::fixed(NonClone(3));
    let state = reactive.create_state(NonClone(4));
    let source = Value::reactive(state.value());
    let computed_source = state.value();
    let computed = reactive.create_computed(move |cx| NonClone(cx.read(&computed_source).0 * 2));
    let computed = Value::reactive(computed.value());
    let inline_source = state.value();
    let inline = Value::inline(move |cx| NonClone(cx.read(&inline_source).0 + 1));
    let reader = reactive.create_computed(move |cx| {
        let fixed_read = fixed.read(cx);
        assert!(std::ptr::eq(&*fixed_read, fixed.as_fixed().unwrap()));
        (
            fixed_read.0,
            source.read(cx).0,
            computed.read(cx).0,
            inline.read(cx).0,
        )
    });
    assert_eq!(*reader.snapshot().unwrap(), (3, 4, 8, 5));
    state.set(NonClone(7)).unwrap();
    assert_eq!(*reader.snapshot().unwrap(), (3, 7, 14, 8));
}

#[test]
fn inline_results_live_in_each_read_and_are_dropped_with_it() {
    struct Temporary {
        id: usize,
        drops: Rc<Cell<usize>>,
    }
    impl Drop for Temporary {
        fn drop(&mut self) {
            self.drops.set(self.drops.get() + 1);
        }
    }
    let reactive = Reactive::new();
    let runs = Rc::new(Cell::new(0));
    let drops = Rc::new(Cell::new(0));
    let count = runs.clone();
    let dropped = drops.clone();
    let value = Value::inline(move |_| {
        count.set(count.get() + 1);
        Temporary {
            id: count.get(),
            drops: dropped.clone(),
        }
    });
    let reader = reactive.create_effect(move |cx| {
        assert!(value.as_fixed().is_none());
        assert_eq!(runs.get(), 0);
        let first = value.read(cx);
        assert_eq!(first.id, 1);
        let second = value.read(cx);
        assert_eq!(second.id, 2);
        assert_eq!(first.id, 1);
        assert_eq!(runs.get(), 2);
        assert_eq!(drops.get(), 0);
        drop(second);
        assert_eq!(drops.get(), 1);
        assert_eq!(first.id, 1);
        drop(first);
        assert_eq!(drops.get(), 2);
    });
    reader.run().unwrap();
}

#[test]
fn fixed_values_are_available_without_evaluation() {
    let reactive = Reactive::new();
    let value = Value::fixed(24);
    assert_eq!(value.as_fixed(), Some(&24));
    let reader = reactive.create_computed(move |cx| *value.read(cx));
    assert_eq!(*reader.snapshot().unwrap(), 24);
}

#[test]
fn computed_retains_results_while_inline_reads_in_each_callers_context() {
    for inline in [false, true] {
        let reactive = Reactive::new();
        let state = reactive.create_state(2);
        let runs = Rc::new(Cell::new(0));
        let count = runs.clone();
        let input = Value::reactive(state.value());
        let read = move |cx: &theoria_reactive::ReadContext<'_>| {
            count.set(count.get() + 1);
            *input.read(cx) * 3
        };
        let value = if inline {
            Value::inline(read)
        } else {
            Value::reactive(reactive.create_computed(move |cx| read(cx)).value())
        };
        assert_eq!(value.as_fixed(), None);
        assert_eq!(runs.get(), 0);
        let other = value.clone();
        let a = reactive.create_effect(move |cx| {
            value.read(cx);
        });
        let b = reactive.create_effect(move |cx| {
            other.read(cx);
        });
        a.run().unwrap();
        b.run().unwrap();
        assert_eq!(runs.get(), if inline { 2 } else { 1 });
        state.set(4).unwrap();
        assert!(a.is_stale());
        assert!(b.is_stale());
        a.run().unwrap();
        b.run().unwrap();
        assert_eq!(runs.get(), if inline { 4 } else { 2 });
    }
}

#[test]
fn result_values_can_be_recovered_by_consumers() {
    for kind in 0..3 {
        let reactive = Reactive::new();
        let state = reactive.create_state(Err::<i32, _>("unavailable"));
        let input = match kind {
            0 => Value::fixed(Err("unavailable")),
            1 => Value::reactive(state.value()),
            _ => {
                let source = Value::reactive(state.value());
                Value::inline(move |cx| *source.read(cx))
            }
        };
        assert_eq!(input.as_fixed(), (kind == 0).then_some(&Err("unavailable")));
        let reader = reactive.create_computed(move |cx| (*input.read(cx)).unwrap_or(-1));
        assert_eq!(*reader.snapshot().unwrap(), -1);
        state.set(Ok(8)).unwrap();
        assert_eq!(*reader.snapshot().unwrap(), if kind == 0 { -1 } else { 8 });
    }
}
