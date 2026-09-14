use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use super::drain;
use theoria_event::{
    Dispatch, DispatchOutcome, DispatchQueue, Dispatcher, EventHandler, EventSource, EventTarget,
    OwningSink, SubmissionOutcome, channel,
};

#[derive(Debug)]
struct LocalDispatch {
    output: Rc<Cell<bool>>,
}

impl Dispatch for LocalDispatch {
    fn run(self) -> DispatchOutcome {
        self.output.set(true);
        DispatchOutcome::Completed
    }
}

#[test]
fn a_local_queue_accepts_a_non_send_static_dispatch() {
    let mut queue = DispatchQueue::new();
    let dispatcher = queue.dispatcher();
    let output = Rc::new(Cell::new(false));

    let outcome = dispatcher
        .submit(LocalDispatch {
            output: Rc::clone(&output),
        })
        .expect("the local queue accepts static local work");

    assert_eq!(outcome, SubmissionOutcome::Deferred);
    assert!(!output.get());
    assert_eq!(queue.run_next(), Some(DispatchOutcome::Completed));
    assert!(output.get());
}

#[test]
fn source_input_and_subscription_use_separate_dispatch_steps() {
    let mut queue = DispatchQueue::new();
    let dispatcher = queue.dispatcher();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));
    let sink = EventTarget::from(&handler);
    let (provider, source) = channel::<u32>();
    source
        .connect(OwningSink::new(sink))
        .via(dispatcher.clone())
        .expect("live provider")
        .detach();

    assert!(
        dispatcher
            .submit(EventTarget::from(&provider).send(7_u32))
            .is_ok()
    );
    assert!(queue.run_next().is_some());
    assert!(values.borrow().is_empty());
    assert!(queue.run_next().is_some());
    assert_eq!(&*values.borrow(), &[7]);
    assert!(queue.run_next().is_none());
}

#[test]
fn an_idle_queue_reports_empty_without_running() {
    let mut queue = DispatchQueue::new();
    let dispatcher = queue.dispatcher();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |event: u32| output.borrow_mut().push(event));
    let sink = EventTarget::from(&handler);

    assert!(queue.is_empty());
    assert!(queue.run_next().is_none());
    assert!(dispatcher.submit(sink.send(1_u32)).is_ok());
    assert!(!queue.is_empty());
    assert!(values.borrow().is_empty());
    assert!(queue.run_next().is_some());
    assert!(queue.is_empty());
    assert_eq!(&*values.borrow(), &[1]);
}

#[test]
fn the_caller_can_stop_between_dispatches_in_a_cyclic_event_chain() {
    let mut queue = DispatchQueue::new();
    let dispatcher = queue.dispatcher();
    let (left_provider, left_source) = channel::<u32>();
    let (right_provider, right_source) = channel::<u32>();
    let _left_to_right = left_source
        .connect(OwningSink::new(EventTarget::from(&right_provider)))
        .via(dispatcher.clone())
        .expect("live provider");
    let _right_to_left = right_source
        .connect(OwningSink::new(EventTarget::from(&left_provider)))
        .via(dispatcher.clone())
        .expect("live provider");

    assert!(
        dispatcher
            .submit(EventTarget::from(&left_provider).send(1_u32))
            .is_ok()
    );
    for _ in 0..8 {
        assert!(queue.run_next().is_some());
    }
}

#[test]
fn dropping_a_dispatch_queue_makes_its_dispatcher_reject_submissions() {
    let queue = DispatchQueue::new();
    let dispatcher = queue.dispatcher();
    let (provider, _source) = channel::<u32>();
    let handler = EventHandler::new(|_: u32| {});
    let independent_sink = EventTarget::from(&handler);
    drop(queue);

    assert!(
        dispatcher
            .submit(EventTarget::from(&provider).send(1_u32))
            .is_err()
    );
    assert_eq!(
        independent_sink.send(1_u32).run(),
        DispatchOutcome::Completed
    );
}

#[test]
fn heterogeneous_dispatch_types_share_one_queue() {
    let mut queue = DispatchQueue::new();
    let dispatcher = queue.dispatcher();
    let order = Rc::new(RefCell::new(Vec::new()));
    let number_order = Rc::clone(&order);
    let text_order = Rc::clone(&order);
    let number_handler = EventHandler::new(move |value: u32| {
        number_order.borrow_mut().push(value.to_string());
    });
    let number_sink = EventTarget::from(&number_handler);
    let text_handler = EventHandler::new(move |value: String| {
        text_order.borrow_mut().push(value);
    });
    let text_sink = EventTarget::from(&text_handler);

    assert!(dispatcher.submit(number_sink.send(1_u32)).is_ok());
    assert!(
        dispatcher
            .submit(text_sink.send(String::from("two")))
            .is_ok()
    );
    drain(&mut queue);

    assert_eq!(&*order.borrow(), &["1", "two"]);
}

#[test]
fn work_submitted_during_a_dispatch_joins_the_queue_tail() {
    let mut queue = DispatchQueue::new();
    let dispatcher = queue.dispatcher();
    let order = Rc::new(RefCell::new(Vec::new()));

    let last_order = Rc::clone(&order);
    let last_handler = EventHandler::new(move |(): ()| {
        last_order.borrow_mut().push("last");
    });
    let last_sink = EventTarget::from(&last_handler);
    let nested_dispatcher = dispatcher.clone();
    let first_order = Rc::clone(&order);
    let first_handler = EventHandler::new(move |(): ()| {
        first_order.borrow_mut().push("first");
        assert!(nested_dispatcher.submit(last_sink.send(())).is_ok());
    });
    let first_sink = EventTarget::from(&first_handler);
    let second_order = Rc::clone(&order);
    let second_handler = EventHandler::new(move |(): ()| {
        second_order.borrow_mut().push("second");
    });
    let second_sink = EventTarget::from(&second_handler);

    assert!(dispatcher.submit(first_sink.send(())).is_ok());
    assert!(dispatcher.submit(second_sink.send(())).is_ok());
    drain(&mut queue);

    assert_eq!(&*order.borrow(), &["first", "second", "last"]);
}
