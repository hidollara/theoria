use std::{cell::RefCell, rc::Rc};

use theoria_event::{EventHandler, EventSink, FifoDispatcher, channel};

#[test]
fn source_input_and_subscription_use_separate_dispatch_steps() {
    let mut dispatcher = FifoDispatcher::new();
    let route = dispatcher.route();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));
    let sink = EventSink::from(&handler);
    let (provider, source) = channel::<u32>();
    source
        .subscribe_cloned(&sink, &route)
        .expect("live provider")
        .detach();

    assert!(route.send(&provider, 7_u32).is_ok());
    assert!(dispatcher.deliver_next());
    assert!(values.borrow().is_empty());
    assert!(dispatcher.deliver_next());
    assert_eq!(&*values.borrow(), &[7]);
    assert!(!dispatcher.deliver_next());
}

#[test]
fn an_idle_dispatcher_reports_idle_without_delivering() {
    let mut dispatcher = FifoDispatcher::new();
    let route = dispatcher.route();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |event: u32| output.borrow_mut().push(event));
    let sink = EventSink::from(&handler);

    assert!(!dispatcher.deliver_next());
    assert!(route.send(&sink, 1_u32).is_ok());
    assert!(values.borrow().is_empty());
    assert!(dispatcher.deliver_next());
    assert_eq!(&*values.borrow(), &[1]);
}

#[test]
fn the_caller_can_stop_between_dispatches_in_a_cyclic_event_chain() {
    let mut dispatcher = FifoDispatcher::new();
    let route = dispatcher.route();
    let (left_provider, left_source) = channel::<u32>();
    let (right_provider, right_source) = channel::<u32>();
    let _left_to_right = left_source
        .subscribe_cloned(&right_provider, &route)
        .expect("live provider");
    let _right_to_left = right_source
        .subscribe_cloned(&left_provider, &route)
        .expect("live provider");

    assert!(route.send(&left_provider, 1_u32).is_ok());
    for _ in 0..8 {
        assert!(dispatcher.deliver_next());
    }
}
