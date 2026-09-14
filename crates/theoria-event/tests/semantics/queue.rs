use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use super::{TestSink, TestSinkOwner, drain};
use theoria_event::{
    Channel, Dispatch, DispatchOutcome, DispatchQueue, Dispatcher, EventSink, SubmissionOutcome,
    Subscription,
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
    let handler = TestSinkOwner::new(move |value: u32| output.borrow_mut().push(value));
    let sink = TestSink::from(&handler);
    let channel = Channel::<u32>::new();
    let source = channel.source();
    Subscription::builder(source, sink)
        .owned()
        .via(dispatcher.clone())
        .expect("live channel")
        .detach();

    assert!(dispatcher.submit(channel.sink().send(7_u32)).is_ok());
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
    let handler = TestSinkOwner::new(move |event: u32| output.borrow_mut().push(event));
    let sink = TestSink::from(&handler);

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
    let left_channel = Channel::<u32>::new();
    let left_source = left_channel.source();
    let right_channel = Channel::<u32>::new();
    let right_source = right_channel.source();
    let _left_to_right = Subscription::builder(left_source, right_channel.sink())
        .owned()
        .via(dispatcher.clone())
        .expect("live channel");
    let _right_to_left = Subscription::builder(right_source, left_channel.sink())
        .owned()
        .via(dispatcher.clone())
        .expect("live channel");

    assert!(dispatcher.submit(left_channel.sink().send(1_u32)).is_ok());
    for _ in 0..8 {
        assert!(queue.run_next().is_some());
    }
}

#[test]
fn dropping_a_dispatch_queue_makes_its_dispatcher_reject_submissions() {
    let queue = DispatchQueue::new();
    let dispatcher = queue.dispatcher();
    let channel = Channel::<u32>::new();
    let _source = channel.source();
    let handler = TestSinkOwner::new(|_: u32| {});
    let independent_sink = TestSink::from(&handler);
    drop(queue);

    assert!(dispatcher.submit(channel.sink().send(1_u32)).is_err());
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
    let number_handler = TestSinkOwner::new(move |value: u32| {
        number_order.borrow_mut().push(value.to_string());
    });
    let number_sink = TestSink::from(&number_handler);
    let text_handler = TestSinkOwner::new(move |value: String| {
        text_order.borrow_mut().push(value);
    });
    let text_sink = TestSink::from(&text_handler);

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
    let last_handler = TestSinkOwner::new(move |(): ()| {
        last_order.borrow_mut().push("last");
    });
    let last_sink = TestSink::from(&last_handler);
    let nested_dispatcher = dispatcher.clone();
    let first_order = Rc::clone(&order);
    let first_handler = TestSinkOwner::new(move |(): ()| {
        first_order.borrow_mut().push("first");
        assert!(nested_dispatcher.submit((&last_sink).send(())).is_ok());
    });
    let first_sink = TestSink::from(&first_handler);
    let second_order = Rc::clone(&order);
    let second_handler = TestSinkOwner::new(move |(): ()| {
        second_order.borrow_mut().push("second");
    });
    let second_sink = TestSink::from(&second_handler);

    assert!(dispatcher.submit(first_sink.send(())).is_ok());
    assert!(dispatcher.submit(second_sink.send(())).is_ok());
    drain(&mut queue);

    assert_eq!(&*order.borrow(), &["first", "second", "last"]);
}
