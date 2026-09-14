use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use super::{TestSink, TestSinkOwner};
use theoria_event::{
    ContramapSink, Dispatch, DispatchOutcome, DispatchQueue, Dispatcher, EventSink, OwningSink,
};

#[test]
fn contramap_runs_when_its_dispatch_runs() {
    let mapped = Rc::new(Cell::new(false));
    let observed_map = Rc::clone(&mapped);
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = TestSinkOwner::new(move |value: usize| output.borrow_mut().push(value));
    let sink = ContramapSink::new(TestSink::from(&handler), move |value: String| {
        observed_map.set(true);
        value.len()
    });

    let dispatch = sink.send(String::from("event"));
    assert!(!mapped.get());
    assert!(values.borrow().is_empty());

    assert_eq!(dispatch.run(), DispatchOutcome::Completed);
    assert!(mapped.get());
    assert_eq!(&*values.borrow(), &[5]);
}

#[test]
fn an_owned_adapter_moves_a_non_clone_mapping_into_its_dispatch() {
    struct NonClone(usize);

    let value = Rc::new(Cell::new(0));
    let output = Rc::clone(&value);
    let handler = TestSinkOwner::new(move |event: usize| output.set(event));
    let offset = NonClone(2);
    let sink = TestSink::from(&handler).contramap(move |event: usize| {
        let NonClone(offset) = offset;
        event + offset
    });

    let dispatch = sink.send(5);

    assert_eq!(dispatch.run(), DispatchOutcome::Completed);
    assert_eq!(value.get(), 7);
}

#[test]
fn contramap_dispatch_retains_a_borrowed_input_until_inline_execution() {
    let length = Rc::new(Cell::new(0));
    let output = Rc::clone(&length);
    let handler = TestSinkOwner::new(move |value: usize| output.set(value));
    let sink = ContramapSink::new(TestSink::from(&handler), str::len);
    let owned = String::from("temporary");
    let dispatch = sink.send(owned.as_str());

    assert_eq!(dispatch.run(), DispatchOutcome::Completed);
    assert_eq!(length.get(), 9);
}

#[test]
fn owning_a_borrowed_input_happens_before_deferred_execution() {
    let mut queue = DispatchQueue::new();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = TestSinkOwner::new(move |value: String| output.borrow_mut().push(value));
    let sink = OwningSink::new(TestSink::from(&handler));
    let input = String::from("temporary");

    queue
        .dispatcher()
        .submit(sink.send(input.as_str()))
        .expect("an owned Dispatch can be deferred");
    drop(input);
    assert!(values.borrow().is_empty());

    assert!(queue.run_next().is_some());
    assert_eq!(&*values.borrow(), &[String::from("temporary")]);
}
