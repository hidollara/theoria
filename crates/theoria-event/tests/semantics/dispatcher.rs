use std::{cell::RefCell, rc::Rc};

use theoria_event::{Dispatcher, EventHandler, EventSink, FifoDispatcher, channel};

#[test]
fn source_input_and_subscription_use_separate_dispatch_steps() {
    let mut dispatcher = FifoDispatcher::new();
    let dispatcher_input: Dispatcher = dispatcher.as_ref().clone();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));
    let sink = EventSink::from(&handler);
    let (provider, source) = channel::<u32>();
    source
        .subscribe_cloned_via(&dispatcher_input, &sink)
        .expect("live provider")
        .detach();

    assert!(dispatcher_input.send(&provider, 7_u32).is_ok());
    assert!(dispatcher.deliver_next());
    assert!(values.borrow().is_empty());
    assert!(dispatcher.deliver_next());
    assert_eq!(&*values.borrow(), &[7]);
    assert!(!dispatcher.deliver_next());
}

#[test]
fn an_idle_dispatcher_reports_idle_without_delivering() {
    let mut dispatcher = FifoDispatcher::new();
    let dispatcher_input: Dispatcher = dispatcher.as_ref().clone();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |event: u32| output.borrow_mut().push(event));
    let sink = EventSink::from(&handler);

    assert!(!dispatcher.has_pending());
    assert!(!dispatcher.deliver_next());
    assert!(dispatcher_input.send(&sink, 1_u32).is_ok());
    assert!(dispatcher.has_pending());
    assert!(values.borrow().is_empty());
    assert!(dispatcher.deliver_next());
    assert!(!dispatcher.has_pending());
    assert_eq!(&*values.borrow(), &[1]);
}

#[test]
fn the_caller_can_stop_between_dispatches_in_a_cyclic_event_chain() {
    let mut dispatcher = FifoDispatcher::new();
    let dispatcher_input: Dispatcher = dispatcher.as_ref().clone();
    let (left_provider, left_source) = channel::<u32>();
    let (right_provider, right_source) = channel::<u32>();
    let _left_to_right = left_source
        .subscribe_cloned_via(&dispatcher_input, &right_provider)
        .expect("live provider");
    let _right_to_left = right_source
        .subscribe_cloned_via(&dispatcher_input, &left_provider)
        .expect("live provider");

    assert!(dispatcher_input.send(&left_provider, 1_u32).is_ok());
    for _ in 0..8 {
        assert!(dispatcher.deliver_next());
    }
}
