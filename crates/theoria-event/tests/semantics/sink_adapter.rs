use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use theoria_event::{
    ContramapSink, Dispatch, DispatchOutcome, Dispatcher, EventHandler, EventSink, EventTarget,
    FifoDispatcher, OwningSink,
};

#[test]
fn contramap_runs_when_its_dispatch_runs() {
    let mapped = Rc::new(Cell::new(false));
    let observed_map = Rc::clone(&mapped);
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: usize| output.borrow_mut().push(value));
    let sink = ContramapSink::new(EventTarget::from(&handler), move |value: String| {
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
fn contramap_dispatch_retains_a_borrowed_input_until_inline_execution() {
    let length = Rc::new(Cell::new(0));
    let output = Rc::clone(&length);
    let handler = EventHandler::new(move |value: usize| output.set(value));
    let sink = ContramapSink::new(EventTarget::from(&handler), str::len);
    let owned = String::from("temporary");
    let dispatch = sink.send(owned.as_str());

    assert_eq!(dispatch.run(), DispatchOutcome::Completed);
    assert_eq!(length.get(), 9);
}

#[test]
fn owning_a_borrowed_input_happens_before_deferred_execution() {
    let mut dispatcher = FifoDispatcher::new();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: String| output.borrow_mut().push(value));
    let sink = OwningSink::new(EventTarget::from(&handler));
    let input = String::from("temporary");

    dispatcher
        .submit(sink.send(input.as_str()))
        .expect("an owned Dispatch can be deferred");
    drop(input);
    assert!(values.borrow().is_empty());

    assert!(dispatcher.deliver_next());
    assert_eq!(&*values.borrow(), &[String::from("temporary")]);
}
