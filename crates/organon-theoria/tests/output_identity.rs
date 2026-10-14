use organon_theoria::{Calculation, ReadContext, Source};
use std::{cell::Cell, rc::Rc};

#[test]
fn fixedness_propagates_through_actual_reads_and_chained_calculations() {
    let reactive = theoria_reactive::Reactive::new();
    let unused = reactive.create_state(100_u32);
    let choose = Source::from(true);
    let fixed = Source::from(3_u32);
    let dynamic: Source<u32> = unused.value().into();
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    let selected = Calculation::new(move |cx| {
        observed.set(observed.get() + 1);
        if *choose.read(cx) {
            *fixed.read(cx)
        } else {
            *dynamic.read(cx)
        }
    })
    .bind(&reactive)
    .unwrap();
    assert_eq!(selected.as_fixed(), Some(&3));
    let derived = Calculation::new(move |cx| *selected.read(cx) * 2)
        .bind(&reactive)
        .unwrap();
    assert_eq!(derived.as_fixed(), Some(&6));
    unused.set(200).unwrap();
    assert_eq!(derived.as_fixed(), Some(&6));
    assert_eq!(calls.get(), 1);
}

#[test]
fn fixed_sources_share_their_data_without_cloning_it() {
    struct Counted(Rc<Cell<usize>>);
    impl Clone for Counted {
        fn clone(&self) -> Self {
            self.0.set(self.0.get() + 1);
            Self(self.0.clone())
        }
    }
    let clones = Rc::new(Cell::new(0));
    let output = Source::from(Counted(clones.clone()));
    let alias = output.clone();
    assert!(std::ptr::eq(
        output.as_fixed().unwrap(),
        alias.as_fixed().unwrap()
    ));
    assert_eq!(clones.get(), 0);
}

#[test]
fn bound_sources_and_forwarding_share_the_computed_result() {
    let reactive = theoria_reactive::Reactive::new();
    let state = reactive.create_state(2_u32);
    let source: Source<u32> = state.value().into();
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    let output = Calculation::new(move |cx| {
        observed.set(observed.get() + 1);
        *source.read(cx) * 2
    })
    .bind(&reactive)
    .unwrap();
    assert_eq!(calls.get(), 1);
    let alias = output.clone();
    let forwarded = Calculation::forward(output.clone())
        .bind(&reactive)
        .unwrap();
    assert_eq!(calls.get(), 1); // Forwarding does not insert another evaluation boundary.
    let combined = reactive.create_computed(move |cx| {
        let cx = ReadContext::new(cx);
        *output.read(&cx) + *alias.read(&cx) + *forwarded.read(&cx)
    });
    assert_eq!(*combined.snapshot().unwrap(), 12);
    assert_eq!(calls.get(), 1); // All readers share the bound computation.
    state.set(3).unwrap();
    assert_eq!(*combined.snapshot().unwrap(), 18);
    assert_eq!(calls.get(), 2);
    // Rust can explicitly share this consumer's computed result.
    let value = combined.value();
    let reader = reactive.create_computed(move |cx| *cx.read(&value) + *cx.read(&value));
    assert_eq!(*reader.snapshot().unwrap(), 36);
    assert_eq!(calls.get(), 2);
}

#[test]
fn reactive_sources_share_the_initial_computed_and_dynamic_dependencies() {
    let reactive = theoria_reactive::Reactive::new();
    let selected = reactive.create_state(true);
    let left = reactive.create_state(2_u32);
    let right = reactive.create_state(10_u32);
    let (choose, a, b): (Source<bool>, Source<u32>, Source<u32>) = (
        selected.value().into(),
        left.value().into(),
        right.value().into(),
    );
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    let output = Calculation::new(move |cx| {
        observed.set(observed.get() + 1);
        if *choose.read(cx) {
            *a.read(cx)
        } else {
            *b.read(cx)
        }
    })
    .bind(&reactive)
    .unwrap();
    let alias = output.clone();
    let reader = reactive.create_computed(move |cx| {
        let cx = ReadContext::new(cx);
        *output.read(&cx) + *alias.read(&cx)
    });
    assert_eq!(*reader.snapshot().unwrap(), 4);
    assert_eq!(calls.get(), 1);
    right.set(20).unwrap();
    assert_eq!(*reader.snapshot().unwrap(), 4);
    assert_eq!(calls.get(), 1);
    selected.set(false).unwrap();
    assert_eq!(*reader.snapshot().unwrap(), 40);
    assert_eq!(calls.get(), 2);
    left.set(3).unwrap();
    assert_eq!(*reader.snapshot().unwrap(), 40);
    assert_eq!(calls.get(), 2);
}
