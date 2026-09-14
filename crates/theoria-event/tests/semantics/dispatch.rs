use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
};

use super::drain;
use theoria_event::{
    DispatchOutcome, DispatchReceipt, DispatchStatus, Dispatcher, EventDispatch, EventDispatcher,
    EventHandler, EventSink, EventSource, FifoDispatcher, SubmissionOutcome, channel,
};

struct TestDispatcher {
    pending: RefCell<VecDeque<EventDispatch<'static>>>,
}

impl EventDispatcher for TestDispatcher {
    fn submit(
        &self,
        dispatch: EventDispatch<'static>,
    ) -> Result<SubmissionOutcome, EventDispatch<'static>> {
        self.pending.borrow_mut().push_back(dispatch);
        Ok(SubmissionOutcome::Deferred)
    }
}

struct RejectingDispatcher;

fn submit_event<E: 'static>(
    dispatcher: &Dispatcher,
    sink: impl Into<EventSink<E>>,
    event: E,
) -> Result<SubmissionOutcome, EventDispatch<'static>> {
    dispatcher.submit(sink.into().send(event))
}

fn submit_tracked<E: 'static>(
    dispatcher: &Dispatcher,
    sink: impl Into<EventSink<E>>,
    event: E,
) -> Result<DispatchReceipt, EventDispatch<'static>> {
    let (dispatch, receipt) = sink.into().send(event).tracked();
    dispatcher.submit(dispatch).map(|_| receipt)
}

impl EventDispatcher for RejectingDispatcher {
    fn submit(
        &self,
        dispatch: EventDispatch<'static>,
    ) -> Result<SubmissionOutcome, EventDispatch<'static>> {
        Err(dispatch)
    }
}

#[test]
fn a_custom_dispatcher_can_defer_an_opaque_dispatch() {
    let dispatcher = Rc::new(TestDispatcher {
        pending: RefCell::new(VecDeque::new()),
    });
    let event_dispatcher = Dispatcher::new(dispatcher.clone());
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));
    let sink = EventSink::from(&handler);

    assert_eq!(
        submit_event(&event_dispatcher, &sink, 7_u32).expect("Dispatcher accepts work"),
        SubmissionOutcome::Deferred
    );
    assert!(values.borrow().is_empty());
    let outcome = dispatcher
        .pending
        .borrow_mut()
        .pop_front()
        .expect("the Dispatch was deferred")
        .run();
    assert_eq!(outcome, DispatchOutcome::Delivered);
    assert_eq!(&*values.borrow(), &[7]);
}

#[test]
fn rejection_returns_the_original_dispatch() {
    let event_dispatcher = Dispatcher::new(RejectingDispatcher);
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));
    let sink = EventSink::from(&handler);

    let dispatch =
        submit_event(&event_dispatcher, &sink, 7_u32).expect_err("the Dispatcher rejects work");
    assert!(values.borrow().is_empty());
    assert_eq!(dispatch.run(), DispatchOutcome::Delivered);
    assert_eq!(&*values.borrow(), &[7]);
}

#[test]
fn tracked_rejection_returns_the_original_dispatch() {
    let event_dispatcher = Dispatcher::new(RejectingDispatcher);
    let handler = EventHandler::new(|_: u32| {});
    let sink = EventSink::from(&handler);

    let dispatch =
        submit_tracked(&event_dispatcher, &sink, 7_u32).expect_err("the Dispatcher rejects work");
    assert_eq!(dispatch.run(), DispatchOutcome::Delivered);
}

#[test]
fn direct_delivery_runs_on_the_sending_stack() {
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));

    assert_eq!(
        EventSink::from(&handler).send(7_u32).run(),
        DispatchOutcome::Delivered
    );
    assert_eq!(&*values.borrow(), &[7]);
}

#[test]
fn a_tracked_dispatch_can_be_cancelled_before_execution() {
    let dispatcher = Rc::new(TestDispatcher {
        pending: RefCell::new(VecDeque::new()),
    });
    let event_dispatcher = Dispatcher::new(dispatcher.clone());
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));
    let sink = EventSink::from(&handler);

    let receipt =
        submit_tracked(&event_dispatcher, &sink, 7_u32).expect("the Dispatcher accepts work");
    assert_eq!(receipt.status(), DispatchStatus::Pending);
    assert!(receipt.cancel());
    assert_eq!(
        receipt.status(),
        DispatchStatus::Finished(DispatchOutcome::Cancelled)
    );
    assert!(!receipt.cancel());
    let outcome = dispatcher
        .pending
        .borrow_mut()
        .pop_front()
        .expect("the Dispatch was deferred")
        .run();
    assert_eq!(outcome, DispatchOutcome::Cancelled);
    assert_eq!(
        receipt.status(),
        DispatchStatus::Finished(DispatchOutcome::Cancelled)
    );
    assert!(values.borrow().is_empty());
}

#[test]
fn execution_reports_when_its_sink_was_dropped() {
    let dispatcher = Rc::new(TestDispatcher {
        pending: RefCell::new(VecDeque::new()),
    });
    let event_dispatcher = Dispatcher::new(dispatcher.clone());
    let handler = EventHandler::new(|_: u32| {});
    let sink = EventSink::from(&handler);

    assert!(submit_event(&event_dispatcher, &sink, 7_u32).is_ok());
    drop(handler);
    let outcome = dispatcher
        .pending
        .borrow_mut()
        .pop_front()
        .expect("the Dispatch was deferred")
        .run();

    assert_eq!(outcome, DispatchOutcome::SinkUnavailable);
}

#[test]
fn dropping_a_receipt_does_not_cancel_its_dispatch() {
    let mut dispatcher = FifoDispatcher::new();
    let event_dispatcher: Dispatcher = dispatcher.as_ref().clone();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));
    let sink = EventSink::from(&handler);

    let receipt =
        submit_tracked(&event_dispatcher, &sink, 7_u32).expect("the Dispatcher accepts work");
    drop(receipt);
    assert!(dispatcher.deliver_next());
    assert_eq!(&*values.borrow(), &[7]);
}

#[test]
fn an_immediate_dispatcher_returns_a_finished_receipt() {
    let handler = EventHandler::new(|_: u32| {});

    let receipt = submit_tracked(&Dispatcher::immediate(), &handler, 7_u32)
        .expect("the immediate Dispatcher accepts the Dispatch");

    assert_eq!(
        receipt.status(),
        DispatchStatus::Finished(DispatchOutcome::Delivered)
    );
    assert!(!receipt.cancel());
}

#[test]
fn a_tracked_deferred_submission_records_its_execution_outcome() {
    let mut dispatcher = FifoDispatcher::new();
    let event_dispatcher: Dispatcher = dispatcher.as_ref().clone();
    let handler = EventHandler::new(|_: u32| {});
    let sink = EventSink::from(&handler);
    let receipt =
        submit_tracked(&event_dispatcher, &sink, 7_u32).expect("the Dispatcher accepts work");

    assert_eq!(receipt.status(), DispatchStatus::Pending);
    assert!(dispatcher.deliver_next());
    assert_eq!(
        receipt.status(),
        DispatchStatus::Finished(DispatchOutcome::Delivered)
    );
}

#[test]
fn a_handler_observes_its_tracked_dispatch_as_executing() {
    let mut dispatcher = FifoDispatcher::new();
    let event_dispatcher: Dispatcher = dispatcher.as_ref().clone();
    let receipt_slot = Rc::new(RefCell::new(None::<DispatchReceipt>));
    let receipt_from_handler = Rc::clone(&receipt_slot);
    let observed = Rc::new(RefCell::new(Vec::new()));
    let observed_from_handler = Rc::clone(&observed);
    let cancellations = Rc::new(RefCell::new(Vec::new()));
    let cancellations_from_handler = Rc::clone(&cancellations);
    let handler = EventHandler::new(move |(): ()| {
        let receipt = receipt_from_handler.borrow();
        let receipt = receipt
            .as_ref()
            .expect("the Receipt is installed before deferred execution");
        let status = receipt.status();
        observed_from_handler.borrow_mut().push(status);
        cancellations_from_handler
            .borrow_mut()
            .push(receipt.cancel());
    });
    let sink = EventSink::from(&handler);
    let receipt =
        submit_tracked(&event_dispatcher, &sink, ()).expect("the Dispatcher accepts the Dispatch");
    receipt_slot.replace(Some(receipt.clone()));

    assert!(dispatcher.deliver_next());

    assert_eq!(&*observed.borrow(), &[DispatchStatus::Executing]);
    assert_eq!(&*cancellations.borrow(), &[false]);
    assert_eq!(
        receipt.status(),
        DispatchStatus::Finished(DispatchOutcome::Delivered)
    );
}

#[test]
fn a_completion_callback_panic_prevents_later_callbacks() {
    let mut dispatcher = FifoDispatcher::new();
    let event_dispatcher: Dispatcher = dispatcher.as_ref().clone();
    let handler = EventHandler::new(|_: u32| {});
    let sink = EventSink::from(&handler);
    let receipt = submit_tracked(&event_dispatcher, &sink, 7_u32)
        .expect("the Dispatcher accepts the Dispatch");
    let later_callback_ran = Rc::new(Cell::new(false));
    receipt.on_complete(|_| panic!("completion panic"));
    let observed = Rc::clone(&later_callback_ran);
    receipt.on_complete(move |_| observed.set(true));

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = dispatcher.deliver_next();
    }));

    assert!(result.is_err());
    assert!(!later_callback_ran.get());
    assert_eq!(
        receipt.status(),
        DispatchStatus::Finished(DispatchOutcome::Delivered)
    );
}

#[test]
fn dropping_a_dispatcher_cancels_its_pending_tracked_dispatches() {
    let dispatcher = FifoDispatcher::new();
    let event_dispatcher: Dispatcher = dispatcher.as_ref().clone();
    let handler = EventHandler::new(|_: u32| {});
    let sink = EventSink::from(&handler);
    let receipt = submit_tracked(&event_dispatcher, &sink, 7_u32)
        .expect("the Dispatcher accepts the Dispatch");
    let outcomes = Rc::new(RefCell::new(Vec::new()));
    let observed = Rc::clone(&outcomes);
    receipt.on_complete(move |outcome| observed.borrow_mut().push(outcome));

    drop(dispatcher);

    assert_eq!(
        receipt.status(),
        DispatchStatus::Finished(DispatchOutcome::Cancelled)
    );
    assert_eq!(&*outcomes.borrow(), &[DispatchOutcome::Cancelled]);
}

#[test]
fn a_panicking_sink_leaves_its_tracked_dispatch_executing() {
    let mut dispatcher = FifoDispatcher::new();
    let event_dispatcher: Dispatcher = dispatcher.as_ref().clone();
    let handler = EventHandler::new(|(): ()| panic!("sink panic"));
    let sink = EventSink::from(&handler);
    let receipt =
        submit_tracked(&event_dispatcher, &sink, ()).expect("the Dispatcher accepts the Dispatch");
    let callbacks = Rc::new(RefCell::new(Vec::new()));
    let observed = Rc::clone(&callbacks);
    receipt.on_complete(move |outcome| observed.borrow_mut().push(outcome));

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = dispatcher.deliver_next();
    }));

    assert!(result.is_err());
    assert_eq!(receipt.status(), DispatchStatus::Executing);
    assert!(callbacks.borrow().is_empty());
}

#[test]
fn a_receipt_notifies_completion_after_deferred_execution() {
    let mut dispatcher = FifoDispatcher::new();
    let event_dispatcher: Dispatcher = dispatcher.as_ref().clone();
    let handler = EventHandler::new(|_: u32| {});
    let sink = EventSink::from(&handler);
    let receipt =
        submit_tracked(&event_dispatcher, &sink, 7_u32).expect("the Dispatcher accepts work");
    let outcomes = Rc::new(RefCell::new(Vec::new()));
    let observed = Rc::clone(&outcomes);

    receipt.on_complete(move |outcome| observed.borrow_mut().push(outcome));
    assert!(outcomes.borrow().is_empty());

    assert!(dispatcher.deliver_next());
    assert_eq!(&*outcomes.borrow(), &[DispatchOutcome::Delivered]);
}

#[test]
fn completion_callbacks_survive_receipt_drop_and_keep_registration_order() {
    let mut dispatcher = FifoDispatcher::new();
    let event_dispatcher: Dispatcher = dispatcher.as_ref().clone();
    let handler = EventHandler::new(|_: u32| {});
    let sink = EventSink::from(&handler);
    let receipt =
        submit_tracked(&event_dispatcher, &sink, 7_u32).expect("the Dispatcher accepts work");
    let notifications = Rc::new(RefCell::new(Vec::new()));
    let first = Rc::clone(&notifications);
    receipt.on_complete(move |outcome| first.borrow_mut().push((1, outcome)));
    let second = Rc::clone(&notifications);
    receipt.on_complete(move |outcome| second.borrow_mut().push((2, outcome)));
    drop(receipt);

    assert!(dispatcher.deliver_next());
    assert_eq!(
        &*notifications.borrow(),
        &[
            (1, DispatchOutcome::Delivered),
            (2, DispatchOutcome::Delivered)
        ]
    );
}

#[test]
fn a_receipt_notifies_immediately_when_already_completed() {
    let handler = EventHandler::new(|_: u32| {});
    let sink = EventSink::from(&handler);
    let receipt = submit_tracked(&Dispatcher::immediate(), &sink, 7_u32)
        .expect("the immediate Dispatcher accepts the Dispatch");
    let outcome = Rc::new(RefCell::new(None));
    let observed = Rc::clone(&outcome);

    receipt.on_complete(move |completed| {
        observed.replace(Some(completed));
    });

    assert_eq!(*outcome.borrow(), Some(DispatchOutcome::Delivered));
}

#[test]
fn cancelling_a_dispatch_notifies_its_completion() {
    let dispatcher = Rc::new(TestDispatcher {
        pending: RefCell::new(VecDeque::new()),
    });
    let event_dispatcher = Dispatcher::new(dispatcher);
    let handler = EventHandler::new(|_: u32| {});
    let sink = EventSink::from(&handler);
    let receipt =
        submit_tracked(&event_dispatcher, &sink, 7_u32).expect("the Dispatcher accepts work");
    let outcome = Rc::new(RefCell::new(None));
    let observed = Rc::clone(&outcome);
    receipt.on_complete(move |completed| {
        observed.replace(Some(completed));
    });

    assert!(receipt.cancel());
    assert_eq!(*outcome.borrow(), Some(DispatchOutcome::Cancelled));
}

#[test]
fn one_sink_can_be_submitted_to_distinct_dispatchers() {
    let mut dispatcher = FifoDispatcher::new();
    let queued: Dispatcher = dispatcher.as_ref().clone();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));
    let sink = EventSink::from(&handler);

    assert_eq!(
        submit_event(&queued, &sink, 1_u32).expect("Dispatcher accepts work"),
        SubmissionOutcome::Deferred
    );
    assert_eq!(
        submit_event(&Dispatcher::immediate(), &sink, 2_u32).expect("Dispatcher accepts work"),
        SubmissionOutcome::Completed(DispatchOutcome::Delivered)
    );
    assert_eq!(&*values.borrow(), &[2]);
    assert!(dispatcher.deliver_next());
    assert_eq!(&*values.borrow(), &[2, 1]);
}

#[test]
fn a_handler_can_build_and_use_a_source_while_handling_one_dispatch() {
    let mut dispatcher = FifoDispatcher::new();
    let event_dispatcher: Dispatcher = dispatcher.as_ref().clone();
    let values = Rc::new(RefCell::new(Vec::new()));
    let dynamic = Rc::new(RefCell::new(None));
    let dynamic_from_handler = Rc::clone(&dynamic);
    let output = Rc::clone(&values);
    let nested_dispatcher = event_dispatcher.clone();
    let trigger_handler = EventHandler::new(move |(): ()| {
        let handler_output = Rc::clone(&output);
        let handler = EventHandler::new(move |value: u32| {
            handler_output.borrow_mut().push(value);
        });
        let sink = EventSink::from(&handler);
        let (provider, source) = channel::<u32>();
        let input = EventSink::from(&provider);
        let subscription = source
            .cloned()
            .connect(sink)
            .via(&nested_dispatcher)
            .expect("live provider");
        dynamic_from_handler.replace(Some((provider, source, handler, subscription)));
        assert!(submit_event(&nested_dispatcher, &input, 9_u32).is_ok());
    });
    let trigger = EventSink::from(&trigger_handler);

    assert!(submit_event(&event_dispatcher, &trigger, ()).is_ok());
    drain(&mut dispatcher);
    assert!(dynamic.borrow().is_some());
    assert_eq!(&*values.borrow(), &[9]);
}

#[test]
fn dropping_a_fifo_dispatcher_makes_its_capability_reject_submissions() {
    let dispatcher = FifoDispatcher::new();
    let event_dispatcher: Dispatcher = dispatcher.as_ref().clone();
    let (provider, _source) = channel::<u32>();
    let handler = EventHandler::new(|_: u32| {});
    let independent_sink = EventSink::from(&handler);
    drop(dispatcher);

    assert!(submit_event(&event_dispatcher, &provider, 1_u32).is_err());
    assert_eq!(
        independent_sink.send(1_u32).run(),
        DispatchOutcome::Delivered
    );
}

#[test]
fn heterogeneous_event_types_share_one_fifo() {
    let mut dispatcher = FifoDispatcher::new();
    let event_dispatcher: Dispatcher = dispatcher.as_ref().clone();
    let order = Rc::new(RefCell::new(Vec::new()));
    let number_order = Rc::clone(&order);
    let text_order = Rc::clone(&order);
    let number_handler = EventHandler::new(move |value: u32| {
        number_order.borrow_mut().push(value.to_string());
    });
    let number_sink = EventSink::from(&number_handler);
    let text_handler = EventHandler::new(move |value: String| {
        text_order.borrow_mut().push(value);
    });
    let text_sink = EventSink::from(&text_handler);

    assert!(submit_event(&event_dispatcher, &number_sink, 1_u32).is_ok());
    assert!(submit_event(&event_dispatcher, &text_sink, String::from("two")).is_ok());
    drain(&mut dispatcher);

    assert_eq!(&*order.borrow(), &["1", "two"]);
}

#[test]
fn work_submitted_while_a_dispatch_executes_joins_the_fifo_tail() {
    let mut dispatcher = FifoDispatcher::new();
    let event_dispatcher: Dispatcher = dispatcher.as_ref().clone();
    let order = Rc::new(RefCell::new(Vec::new()));

    let last_order = Rc::clone(&order);
    let last_handler = EventHandler::new(move |(): ()| {
        last_order.borrow_mut().push("last");
    });
    let last_sink = EventSink::from(&last_handler);
    let nested_dispatcher = event_dispatcher.clone();
    let first_order = Rc::clone(&order);
    let first_handler = EventHandler::new(move |(): ()| {
        first_order.borrow_mut().push("first");
        assert!(submit_event(&nested_dispatcher, &last_sink, ()).is_ok());
    });
    let first_sink = EventSink::from(&first_handler);
    let second_order = Rc::clone(&order);
    let second_handler = EventHandler::new(move |(): ()| {
        second_order.borrow_mut().push("second");
    });
    let second_sink = EventSink::from(&second_handler);

    assert!(submit_event(&event_dispatcher, &first_sink, ()).is_ok());
    assert!(submit_event(&event_dispatcher, &second_sink, ()).is_ok());
    drain(&mut dispatcher);

    assert_eq!(&*order.borrow(), &["first", "second", "last"]);
}
