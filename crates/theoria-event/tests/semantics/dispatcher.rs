use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use theoria_event::{
    Dispatch, DispatchOutcome, Dispatcher, EventDispatcher, EventHandler, EventSource, EventTarget,
    FifoDispatcher, ImmediateDispatcher, OwningSink, SubmissionOutcome, channel,
};

#[derive(Debug)]
struct BorrowedDispatch<'value> {
    output: &'value Cell<bool>,
}

impl Dispatch for BorrowedDispatch<'_> {
    fn run(self) -> DispatchOutcome {
        self.output.set(true);
        DispatchOutcome::Delivered
    }
}

#[derive(Debug)]
struct LocalDispatch {
    output: Rc<Cell<bool>>,
}

impl Dispatch for LocalDispatch {
    fn run(self) -> DispatchOutcome {
        self.output.set(true);
        DispatchOutcome::Delivered
    }
}

#[test]
fn an_immediate_dispatcher_accepts_a_borrowed_dispatch() {
    let output = Cell::new(false);

    let outcome = ImmediateDispatcher
        .submit(BorrowedDispatch { output: &output })
        .expect("an immediate Dispatcher accepts borrowed work");

    assert_eq!(
        outcome,
        SubmissionOutcome::Completed(DispatchOutcome::Delivered)
    );
    assert!(output.get());
}

#[test]
fn a_local_fifo_accepts_a_non_send_static_dispatch() {
    let mut dispatcher = FifoDispatcher::new();
    let output = Rc::new(Cell::new(false));

    let outcome = dispatcher
        .submit(LocalDispatch {
            output: Rc::clone(&output),
        })
        .expect("a local FIFO accepts static local work");

    assert_eq!(outcome, SubmissionOutcome::Deferred);
    assert!(!output.get());
    assert!(dispatcher.deliver_next());
    assert!(output.get());
}

#[test]
fn source_input_and_subscription_use_separate_dispatch_steps() {
    let mut dispatcher = FifoDispatcher::new();
    let dispatcher_input: Dispatcher = dispatcher.as_ref().clone();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));
    let sink = EventTarget::from(&handler);
    let (provider, source) = channel::<u32>();
    source
        .connect(OwningSink::new(sink))
        .via(dispatcher_input.clone())
        .expect("live provider")
        .detach();

    assert!(
        dispatcher_input
            .submit(EventTarget::from(&provider).send(7_u32))
            .is_ok()
    );
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
    let sink = EventTarget::from(&handler);

    assert!(!dispatcher.has_pending());
    assert!(!dispatcher.deliver_next());
    assert!(dispatcher_input.submit(sink.send(1_u32)).is_ok());
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
        .connect(OwningSink::new(EventTarget::from(&right_provider)))
        .via(dispatcher_input.clone())
        .expect("live provider");
    let _right_to_left = right_source
        .connect(OwningSink::new(EventTarget::from(&left_provider)))
        .via(dispatcher_input.clone())
        .expect("live provider");

    assert!(
        dispatcher_input
            .submit(EventTarget::from(&left_provider).send(1_u32))
            .is_ok()
    );
    for _ in 0..8 {
        assert!(dispatcher.deliver_next());
    }
}
