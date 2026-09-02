use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
};

use super::drain;
use theoria_event::{
    DispatchOutcome, DispatchReceipt, DispatchRoute, DispatchStatus, EventDispatch,
    EventDispatcher, EventHandler, EventSink, FifoDispatcher, SubmissionOutcome, channel,
};

struct TestDispatcher {
    pending: RefCell<VecDeque<EventDispatch>>,
}

impl EventDispatcher for TestDispatcher {
    fn submit(&self, dispatch: EventDispatch) -> Result<SubmissionOutcome, EventDispatch> {
        self.pending.borrow_mut().push_back(dispatch);
        Ok(SubmissionOutcome::Deferred)
    }
}

struct RejectingDispatcher;

impl EventDispatcher for RejectingDispatcher {
    fn submit(&self, dispatch: EventDispatch) -> Result<SubmissionOutcome, EventDispatch> {
        Err(dispatch)
    }
}

#[test]
fn a_custom_dispatcher_can_defer_an_opaque_dispatch() {
    let dispatcher = Rc::new(TestDispatcher {
        pending: RefCell::new(VecDeque::new()),
    });
    let route = DispatchRoute::Dispatcher(dispatcher.clone());
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));
    let sink = EventSink::from(&handler);

    assert_eq!(route.send(&sink, 7_u32), Ok(SubmissionOutcome::Deferred));
    assert!(values.borrow().is_empty());
    let outcome = dispatcher
        .pending
        .borrow_mut()
        .pop_front()
        .expect("the Dispatch was deferred")
        .execute();
    assert_eq!(outcome, DispatchOutcome::Delivered);
    assert_eq!(&*values.borrow(), &[7]);
}

#[test]
fn rejection_returns_the_original_event() {
    let route = DispatchRoute::Dispatcher(Rc::new(RejectingDispatcher));
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));
    let sink = EventSink::from(&handler);

    let event = route
        .send(&sink, 7_u32)
        .expect_err("the Dispatcher rejects work");
    assert!(values.borrow().is_empty());
    assert_eq!(event, 7);
}

#[test]
fn tracked_rejection_returns_the_original_event() {
    let route = DispatchRoute::Dispatcher(Rc::new(RejectingDispatcher));
    let handler = EventHandler::new(|_: u32| {});
    let sink = EventSink::from(&handler);

    let event = route
        .send_tracked(&sink, 7_u32)
        .expect_err("the Dispatcher rejects work");

    assert_eq!(event, 7);
}

#[test]
fn inline_dispatch_runs_on_the_submitting_stack() {
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));

    assert_eq!(
        DispatchRoute::Inline.send(&handler, 7_u32),
        Ok(SubmissionOutcome::Completed(DispatchOutcome::Delivered))
    );
    assert_eq!(&*values.borrow(), &[7]);
}

#[test]
fn a_tracked_dispatch_can_be_cancelled_before_execution() {
    let dispatcher = Rc::new(TestDispatcher {
        pending: RefCell::new(VecDeque::new()),
    });
    let route = DispatchRoute::Dispatcher(dispatcher.clone());
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));
    let sink = EventSink::from(&handler);

    let receipt = route
        .send_tracked(&sink, 7_u32)
        .expect("the Dispatcher accepts work");
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
        .execute();
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
    let route = DispatchRoute::Dispatcher(dispatcher.clone());
    let handler = EventHandler::new(|_: u32| {});
    let sink = EventSink::from(&handler);

    assert!(route.send(&sink, 7_u32).is_ok());
    drop(handler);
    let outcome = dispatcher
        .pending
        .borrow_mut()
        .pop_front()
        .expect("the Dispatch was deferred")
        .execute();

    assert_eq!(outcome, DispatchOutcome::SinkUnavailable);
}

#[test]
fn dropping_a_receipt_does_not_cancel_its_dispatch() {
    let mut dispatcher = FifoDispatcher::new();
    let route = dispatcher.route();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));
    let sink = EventSink::from(&handler);

    let receipt = route
        .send_tracked(&sink, 7_u32)
        .expect("the Dispatcher accepts work");
    drop(receipt);
    assert!(dispatcher.deliver_next());
    assert_eq!(&*values.borrow(), &[7]);
}

#[test]
fn a_tracked_inline_submission_returns_a_finished_receipt() {
    let handler = EventHandler::new(|_: u32| {});

    let receipt = DispatchRoute::Inline
        .send_tracked(&handler, 7_u32)
        .expect("Inline accepts the Dispatch");

    assert_eq!(
        receipt.status(),
        DispatchStatus::Finished(DispatchOutcome::Delivered)
    );
    assert!(!receipt.cancel());
}

#[test]
fn a_tracked_deferred_submission_records_its_execution_outcome() {
    let mut dispatcher = FifoDispatcher::new();
    let route = dispatcher.route();
    let handler = EventHandler::new(|_: u32| {});
    let sink = EventSink::from(&handler);
    let receipt = route
        .send_tracked(&sink, 7_u32)
        .expect("the Dispatcher accepts work");

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
    let route = dispatcher.route();
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
    let receipt = route
        .send_tracked(&sink, ())
        .expect("the Dispatcher accepts the Dispatch");
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
    let route = dispatcher.route();
    let handler = EventHandler::new(|_: u32| {});
    let sink = EventSink::from(&handler);
    let receipt = route
        .send_tracked(&sink, 7_u32)
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
    let route = dispatcher.route();
    let handler = EventHandler::new(|_: u32| {});
    let sink = EventSink::from(&handler);
    let receipt = route
        .send_tracked(&sink, 7_u32)
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
    let route = dispatcher.route();
    let handler = EventHandler::new(|(): ()| panic!("sink panic"));
    let sink = EventSink::from(&handler);
    let receipt = route
        .send_tracked(&sink, ())
        .expect("the Dispatcher accepts the Dispatch");
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
    let route = dispatcher.route();
    let handler = EventHandler::new(|_: u32| {});
    let sink = EventSink::from(&handler);
    let receipt = route
        .send_tracked(&sink, 7_u32)
        .expect("the Dispatcher accepts work");
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
    let route = dispatcher.route();
    let handler = EventHandler::new(|_: u32| {});
    let sink = EventSink::from(&handler);
    let receipt = route
        .send_tracked(&sink, 7_u32)
        .expect("the Dispatcher accepts work");
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
    let receipt = DispatchRoute::Inline
        .send_tracked(&sink, 7_u32)
        .expect("Inline accepts the Dispatch");
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
    let route = DispatchRoute::Dispatcher(dispatcher);
    let handler = EventHandler::new(|_: u32| {});
    let sink = EventSink::from(&handler);
    let receipt = route
        .send_tracked(&sink, 7_u32)
        .expect("the Dispatcher accepts work");
    let outcome = Rc::new(RefCell::new(None));
    let observed = Rc::clone(&outcome);
    receipt.on_complete(move |completed| {
        observed.replace(Some(completed));
    });

    assert!(receipt.cancel());
    assert_eq!(*outcome.borrow(), Some(DispatchOutcome::Cancelled));
}

#[test]
fn one_sink_can_be_combined_with_distinct_routes() {
    let mut dispatcher = FifoDispatcher::new();
    let queued = dispatcher.route();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));
    let sink = EventSink::from(&handler);

    assert_eq!(queued.send(&sink, 1_u32), Ok(SubmissionOutcome::Deferred));
    assert_eq!(
        DispatchRoute::Inline.send(&sink, 2_u32),
        Ok(SubmissionOutcome::Completed(DispatchOutcome::Delivered))
    );
    assert_eq!(&*values.borrow(), &[2]);
    assert!(dispatcher.deliver_next());
    assert_eq!(&*values.borrow(), &[2, 1]);
}

#[test]
fn a_handler_can_build_and_use_a_source_while_handling_one_dispatch() {
    let mut dispatcher = FifoDispatcher::new();
    let route = dispatcher.route();
    let values = Rc::new(RefCell::new(Vec::new()));
    let dynamic = Rc::new(RefCell::new(None));
    let dynamic_from_handler = Rc::clone(&dynamic);
    let output = Rc::clone(&values);
    let nested_route = route.clone();
    let trigger_handler = EventHandler::new(move |(): ()| {
        let handler_output = Rc::clone(&output);
        let handler = EventHandler::new(move |value: u32| {
            handler_output.borrow_mut().push(value);
        });
        let sink = EventSink::from(&handler);
        let (provider, source) = channel::<u32>();
        let input = EventSink::from(&provider);
        let subscription = source
            .subscribe_cloned(&sink, &nested_route)
            .expect("live provider");
        dynamic_from_handler.replace(Some((provider, source, handler, subscription)));
        assert!(nested_route.send(&input, 9_u32).is_ok());
    });
    let trigger = EventSink::from(&trigger_handler);

    assert!(route.send(&trigger, ()).is_ok());
    drain(&mut dispatcher);
    assert!(dynamic.borrow().is_some());
    assert_eq!(&*values.borrow(), &[9]);
}

#[test]
fn dropping_the_dispatcher_makes_its_route_reject_submissions() {
    let dispatcher = FifoDispatcher::new();
    let route = dispatcher.route();
    let (provider, _source) = channel::<u32>();
    let handler = EventHandler::new(|_: u32| {});
    let independent_sink = EventSink::from(&handler);
    drop(dispatcher);

    assert!(route.send(&provider, 1_u32).is_err());
    assert!(DispatchRoute::Inline.send(&independent_sink, 1_u32).is_ok());
}

#[test]
fn heterogeneous_event_types_share_one_fifo() {
    let mut dispatcher = FifoDispatcher::new();
    let route = dispatcher.route();
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

    assert!(route.send(&number_sink, 1_u32).is_ok());
    assert!(route.send(&text_sink, String::from("two")).is_ok());
    drain(&mut dispatcher);

    assert_eq!(&*order.borrow(), &["1", "two"]);
}

#[test]
fn work_submitted_while_a_dispatch_executes_joins_the_fifo_tail() {
    let mut dispatcher = FifoDispatcher::new();
    let route = dispatcher.route();
    let order = Rc::new(RefCell::new(Vec::new()));

    let last_order = Rc::clone(&order);
    let last_handler = EventHandler::new(move |(): ()| {
        last_order.borrow_mut().push("last");
    });
    let last_sink = EventSink::from(&last_handler);
    let nested_route = route.clone();
    let first_order = Rc::clone(&order);
    let first_handler = EventHandler::new(move |(): ()| {
        first_order.borrow_mut().push("first");
        assert!(nested_route.send(&last_sink, ()).is_ok());
    });
    let first_sink = EventSink::from(&first_handler);
    let second_order = Rc::clone(&order);
    let second_handler = EventHandler::new(move |(): ()| {
        second_order.borrow_mut().push("second");
    });
    let second_sink = EventSink::from(&second_handler);

    assert!(route.send(&first_sink, ()).is_ok());
    assert!(route.send(&second_sink, ()).is_ok());
    drain(&mut dispatcher);

    assert_eq!(&*order.borrow(), &["first", "second", "last"]);
}
